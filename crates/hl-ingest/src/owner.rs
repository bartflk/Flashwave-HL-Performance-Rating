//! The owner's name and profile picture, for the top of the window.
//!
//! No Steam login and no API key: the ETF2L profile fetched on every sync
//! already carries the Steam name and avatar link, and anyone without an
//! ETF2L account falls back to Steam's public profile XML
//! (`steamcommunity.com/profiles/<id>?xml=1`). A private Steam profile gets
//! no picture. The image is downloaded once and kept in the database as a
//! data URL, so it shows offline and needs no outside image source in the
//! app's security policy.

use crate::sources::Sources;
use anyhow::Result;
use base64::Engine;
use hl_core::SteamId;
use hl_db::Db;
use serde::Serialize;

const NAME: &str = "owner_name";
const AVATAR: &str = "owner_avatar";
/// The URL the stored picture came from: fetched again only when it changes.
const AVATAR_SRC: &str = "owner_avatar_src";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub steamid64: String,
    pub name: Option<String>,
    /// A `data:` URL, ready for an `<img>`.
    pub avatar: Option<String>,
}

/// What is stored now; no network.
pub async fn load(db: &Db, me: SteamId) -> Result<Owner> {
    Ok(Owner {
        steamid64: me.to_steamid64(),
        name: db.get_setting(NAME).await?,
        avatar: db.get_setting(AVATAR).await?,
    })
}

/// Look the owner up and store their name and picture. Best effort: a
/// failure leaves what was stored before.
pub async fn refresh(db: &Db, sources: &Sources, me: SteamId) -> Result<Owner> {
    let (name, src) = match from_etf2l(db, me).await? {
        Some(found) => found,
        None => from_steam(sources, me).await?.unwrap_or((None, None)),
    };
    if let Some(name) = name.as_deref().filter(|n| !n.is_empty()) {
        db.set_setting(NAME, name).await?;
    }
    if let Some(src) = src {
        let stored = db.get_setting(AVATAR_SRC).await?;
        if stored.as_deref() != Some(src.as_str()) || db.get_setting(AVATAR).await?.is_none() {
            if let Some(bytes) = sources.fetch_bytes(&src).await? {
                db.set_setting(AVATAR, &data_url(&bytes)).await?;
                db.set_setting(AVATAR_SRC, &src).await?;
            }
        }
    }
    load(db, me).await
}

/// Name and avatar link from the stored ETF2L profile, when it is the owner's.
async fn from_etf2l(db: &Db, me: SteamId) -> Result<Option<(Option<String>, Option<String>)>> {
    for (_, _, json) in db.etf2l_raw("player").await? {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { continue };
        let p = v.get("player").unwrap_or(&v);
        let steam = &p["steam"];
        if steam["id64"].as_str() != Some(me.to_steamid64().as_str()) {
            continue;
        }
        let name = p["name"].as_str().map(str::to_string);
        let avatar = steam["avatar"].as_str().filter(|a| a.starts_with("https://")).map(str::to_string);
        return Ok(Some((name, avatar)));
    }
    Ok(None)
}

/// Name and avatar link from Steam's public profile XML.
async fn from_steam(sources: &Sources, me: SteamId) -> Result<Option<(Option<String>, Option<String>)>> {
    let url = format!("https://steamcommunity.com/profiles/{}/?xml=1", me.to_steamid64());
    let Some(xml) = sources.fetch_text(&url).await? else { return Ok(None) };
    let name = xml_value(&xml, "steamID");
    let avatar = xml_value(&xml, "avatarFull").filter(|a| a.starts_with("https://"));
    Ok(Some((name, avatar)))
}

/// `<tag><![CDATA[value]]></tag>` or `<tag>value</tag>`.
fn xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    let v = xml[start..end].trim();
    let v = v.strip_prefix("<![CDATA[").and_then(|v| v.strip_suffix("]]>")).unwrap_or(v);
    (!v.trim().is_empty()).then(|| v.trim().to_string())
}

fn data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else {
        "image/jpeg"
    };
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_profile_xml() {
        let xml = "<profile><steamID64>1</steamID64><steamID><![CDATA[Flashy]]></steamID>\
                   <avatarFull><![CDATA[https://avatars.steamstatic.com/x_full.jpg]]></avatarFull></profile>";
        assert_eq!(xml_value(xml, "steamID").as_deref(), Some("Flashy"));
        assert_eq!(xml_value(xml, "avatarFull").as_deref(), Some("https://avatars.steamstatic.com/x_full.jpg"));
        assert_eq!(xml_value(xml, "missing"), None);
    }

    #[test]
    fn data_urls_name_the_image_type() {
        assert!(data_url(b"\x89PNG\r\n").starts_with("data:image/png;base64,"));
        assert!(data_url(&[0xFF, 0xD8, 0xFF]).starts_with("data:image/jpeg;base64,"));
    }
}
