/**
 * Shared by the ⌘D `DatabasePicker` and the palette's Databases entries: switching
 * to the database you're already on would wipe every open tab for nothing.
 */
export function shouldSwitchDatabase(db: string, activeDatabase: string | null | undefined): boolean {
  return db !== activeDatabase;
}
