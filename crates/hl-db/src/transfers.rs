//! ETF2L transfers (Q48): every join and leave, stored as read. Plain
//! loaders; what they mean for medals, team pages and profiles is worked out
//! in `hl_ingest::transfers`.

use crate::Db;
use anyhow::Result;
use sqlx::Row;

/// One join or leave, as ETF2L lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub team_id: i64,
    pub team_name: String,
    pub team_type: Option<String>,
    pub player_id: i64,
    pub account_id: Option<u32>,
    pub player_name: String,
    /// "joined" or "left".
    pub kind: String,
    pub time: i64,
    /// Who made the change: the player themselves, or a leader.
    pub by_id: Option<i64>,
    pub by_name: Option<String>,
}

impl Transfer {
    pub fn joined(&self) -> bool {
        self.kind == "joined"
    }
}

const COLUMNS: &str = "team_id, team_name, team_type, player_id, account_id, player_name, kind, time, by_id, by_name";

fn from_row(r: &sqlx::sqlite::SqliteRow) -> Transfer {
    Transfer {
        team_id: r.get("team_id"),
        team_name: r.get("team_name"),
        team_type: r.get("team_type"),
        player_id: r.get("player_id"),
        account_id: r.get::<Option<i64>, _>("account_id").map(|a| a as u32),
        player_name: r.get("player_name"),
        kind: r.get("kind"),
        time: r.get("time"),
        by_id: r.get("by_id"),
        by_name: r.get("by_name"),
    }
}

impl Db {
    /// Store transfers; returns how many were new.
    pub async fn put_transfers(&self, rows: &[Transfer]) -> Result<usize> {
        let mut tx = self.pool().begin().await?;
        let mut added = 0;
        for t in rows {
            let r = sqlx::query(&format!("INSERT OR IGNORE INTO etf2l_transfer ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"))
                .bind(t.team_id)
                .bind(&t.team_name)
                .bind(&t.team_type)
                .bind(t.player_id)
                .bind(t.account_id.map(i64::from))
                .bind(&t.player_name)
                .bind(&t.kind)
                .bind(t.time)
                .bind(t.by_id)
                .bind(&t.by_name)
                .execute(&mut *tx)
                .await?;
            added += r.rows_affected() as usize;
        }
        tx.commit().await?;
        Ok(added)
    }

    /// When a team's ('team') or a player's ('player') list was last read.
    pub async fn transfers_fetched(&self, kind: &str, id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT fetched_at FROM etf2l_transfer_fetch WHERE kind = ?1 AND id = ?2")
            .bind(kind)
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn set_transfers_fetched(&self, kind: &str, id: i64, at: i64) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO etf2l_transfer_fetch (kind, id, fetched_at) VALUES (?1, ?2, ?3)")
            .bind(kind)
            .bind(id)
            .bind(at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A team's transfers, oldest first.
    pub async fn team_transfers(&self, team: i64) -> Result<Vec<Transfer>> {
        let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM etf2l_transfer WHERE team_id = ?1 ORDER BY time, kind DESC"))
            .bind(team)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(from_row).collect())
    }

    /// A player's transfers over every team, oldest first.
    pub async fn account_transfers(&self, account: u32) -> Result<Vec<Transfer>> {
        let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM etf2l_transfer WHERE account_id = ?1 ORDER BY time, kind DESC"))
            .bind(i64::from(account))
            .fetch_all(self.pool())
            .await?;
        Ok(rows.iter().map(from_row).collect())
    }

    /// Every transfer with a Steam account, for the medals:
    /// `(team, account, time, joined)`, oldest first.
    pub async fn catalogue_transfers(&self) -> Result<Vec<(i64, u32, i64, bool)>> {
        let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT team_id, account_id, time, kind FROM etf2l_transfer WHERE account_id IS NOT NULL ORDER BY time, kind DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(t, a, at, k)| (t, a as u32, at, k == "joined")).collect())
    }

    /// Teams that played Highlander officials and whose transfers were never
    /// read, with their newest official's time.
    pub async fn teams_without_transfers(&self) -> Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT team, MAX(time) FROM (
                 SELECT clan1_id AS team, time FROM etf2l_season_match
                 UNION ALL SELECT clan2_id, time FROM etf2l_season_match)
             WHERE team NOT IN (SELECT id FROM etf2l_transfer_fetch WHERE kind = 'team')
             GROUP BY team",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// Teams that played an official since `since`, and when their transfers
    /// were last read (`None`: never).
    pub async fn active_teams_transfers(&self, since: i64) -> Result<Vec<(i64, Option<i64>)>> {
        Ok(sqlx::query_as(
            "SELECT t.team, f.fetched_at FROM (
                 SELECT clan1_id AS team FROM etf2l_season_match WHERE time >= ?1
                 UNION SELECT clan2_id FROM etf2l_season_match WHERE time >= ?1) t
             LEFT JOIN etf2l_transfer_fetch f ON f.kind = 'team' AND f.id = t.team",
        )
        .bind(since)
        .fetch_all(self.pool())
        .await?)
    }

    /// The newest official a team played, if any.
    /// When a player last played an official the app holds (the league
    /// sample's logs), for how often their ETF2L lists are read again.
    pub async fn player_last_official(&self, account: u32) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT MAX(l.played_at) FROM league_log_player p JOIN league_log l ON l.log_id = p.log_id WHERE p.account_id = ?1",
        )
        .bind(i64::from(account))
        .fetch_one(self.pool())
        .await?)
    }

    /// When a player last played any game the app holds: the league
    /// sample's logs and the owner's.
    pub async fn player_last_game(&self, account: u32) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT MAX(t) FROM (
                SELECT l.played_at AS t FROM league_log_player p JOIN league_log l ON l.log_id = p.log_id WHERE p.account_id = ?1
                UNION ALL
                SELECT i.played_at FROM log_stat s JOIN log_index i ON i.log_id = s.log_id WHERE s.account_id = ?1)",
        )
        .bind(i64::from(account))
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn team_last_official(&self, team: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT MAX(time) FROM etf2l_season_match WHERE clan1_id = ?1 OR clan2_id = ?1")
            .bind(team)
            .fetch_one(self.pool())
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(team: i64, player: i64, account: u32, kind: &str, time: i64) -> Transfer {
        Transfer {
            team_id: team,
            team_name: format!("team {team}"),
            team_type: Some("Highlander".into()),
            player_id: player,
            account_id: Some(account),
            player_name: format!("p{player}"),
            kind: kind.into(),
            time,
            by_id: None,
            by_name: None,
        }
    }

    #[tokio::test]
    async fn transfers_are_stored_once_and_read_back_in_order() {
        let db = Db::connect_in_memory().await.unwrap();
        let rows = vec![t(1, 10, 100, "left", 300), t(1, 10, 100, "joined", 100), t(2, 10, 100, "joined", 400)];
        assert_eq!(db.put_transfers(&rows).await.unwrap(), 3);
        // The same page read again adds nothing.
        assert_eq!(db.put_transfers(&rows).await.unwrap(), 0);
        let team: Vec<i64> = db.team_transfers(1).await.unwrap().iter().map(|x| x.time).collect();
        assert_eq!(team, vec![100, 300]);
        assert_eq!(db.account_transfers(100).await.unwrap().len(), 3);
        assert_eq!(db.catalogue_transfers().await.unwrap()[0], (1, 100, 100, true));
        assert_eq!(db.transfers_fetched("team", 1).await.unwrap(), None);
        db.set_transfers_fetched("team", 1, 55).await.unwrap();
        assert_eq!(db.transfers_fetched("team", 1).await.unwrap(), Some(55));
    }
}
