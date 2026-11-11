// Stands in for src/api/client.ts, which talks to Tauri and reads Vite's
// import.meta.env at load. The pure logic under test never calls it.
export const inTauri = false;
export const api: Record<string, never> = {};
