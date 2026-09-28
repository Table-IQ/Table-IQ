//! Native macOS menu bar: the default App/Edit/Window menus (so ⌘C/⌘V/⌘Z keep working in
//! the webview) plus View and Help. Only keys the editor/webview don't own get a native
//! accelerator (see `accelerator`) — ⌘F/⌘[/⌘] stay with CodeMirror. Each item emits
//! `tiq:menu` with its id (src/lib/menuEvents.ts routes it to the shortcut's handler).

use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Runtime};

/// Native accelerator for an item, only where no webview/editor binding conflicts. Never
/// ⌘F, ⌘[, ⌘] (CodeMirror), ⌘⇧\ (CodeMirror's `Shift-Mod-\` cursorMatchingBracket) or
/// ⌘⇧C: those stay on the webview keymap, which lets the editor win via `defaultPrevented`.
/// The webview keymap skips the three below in the Tauri build (`shortcutFor`'s `native`).
pub fn accelerator(id: &str) -> Option<&'static str> {
    match id {
        "view.palette" => Some("CmdOrCtrl+K"),
        "view.data" => Some("Alt+CmdOrCtrl+1"),
        "view.structure" => Some("Alt+CmdOrCtrl+2"),
        _ => None,
    }
}

/// (id, label) of the View menu's plain items, in order. Kept in sync with MENU_IDS in
/// src/lib/menuEvents.ts (unit test below).
pub fn view_items() -> Vec<(&'static str, &'static str)> {
    vec![
        ("view.sidebar", "Toggle Sidebar"),
        ("view.inspector", "Toggle Inspector"),
        ("view.filter", "Filter Rows…"),
        ("view.data", "Data"),
        ("view.structure", "Structure"),
        ("view.palette", "Command Palette"),
        ("view.sqllog", "Toggle SQL Log"),
        ("view.overview", "Workspace Overview"),
    ]
}

/// View → Appearance submenu items.
const APPEARANCE_ITEMS: [(&str, &str); 3] = [
    ("view.theme.light", "Light"),
    ("view.theme.dark", "Dark"),
    ("view.theme.system", "System"),
];

/// Appended to the default Help menu.
const TOUR_ITEM: (&str, &str) = ("help.tour", "Show Tour");

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // Menu::default already has View (macOS: Enter Full Screen) and Help (the macOS Help
    // search field); fill those instead of adding a second of each.
    let menu = Menu::default(app)?;
    let item = |(id, text): (&str, &str)| MenuItem::with_id(app, id, text, true, accelerator(id));

    let view = match menu
        .items()?
        .iter()
        .find_map(|k| k.as_submenu().filter(|s| s.text().ok().as_deref() == Some("View")).cloned())
    {
        Some(v) => v,
        // No default View off macOS: add one before Window and Help (the last two).
        None => {
            let v = Submenu::new(app, "View", true)?;
            menu.insert(&v, menu.items()?.len().saturating_sub(2))?;
            v
        }
    };

    let appearance_items =
        APPEARANCE_ITEMS.into_iter().map(item).collect::<tauri::Result<Vec<_>>>()?;
    let appearance_refs: Vec<&dyn IsMenuItem<R>> =
        appearance_items.iter().map(|i| i as &dyn IsMenuItem<R>).collect();
    let appearance = Submenu::with_items(app, "Appearance", true, &appearance_refs)?;

    // Groups: sidebar/inspector/filter | data/structure | palette/sqllog/overview/appearance.
    let mut ours: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    for (i, entry) in view_items().into_iter().enumerate() {
        if matches!(i, 3 | 5) {
            ours.push(Box::new(PredefinedMenuItem::separator(app)?));
        }
        ours.push(Box::new(item(entry)?));
    }
    ours.push(Box::new(appearance));
    if !view.items()?.is_empty() {
        ours.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    let refs: Vec<&dyn IsMenuItem<R>> = ours.iter().map(|b| b.as_ref()).collect();
    view.prepend_items(&refs)?; // Enter Full Screen stays last

    if let Some(help) = menu.get(HELP_SUBMENU_ID).and_then(|k| k.as_submenu().cloned()) {
        help.append(&item(TOUR_ITEM)?)?;
    }
    Ok(menu)
}

/// Forward our items to the webview; predefined items (copy, quit, …) are handled natively.
pub fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().0.as_str();
    if id.starts_with("view.") || id.starts_with("help.") {
        if let Err(e) = app.emit("tiq:menu", id) {
            eprintln!("[menu] emit {id} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn view_ids_match_the_frontend_contract() {
        let ts = include_str!("../../src/lib/menuEvents.ts");
        let ids: Vec<&str> = super::view_items().into_iter().map(|(id, _)| id).collect();
        assert_eq!(
            ids,
            [
                "view.sidebar",
                "view.inspector",
                "view.filter",
                "view.data",
                "view.structure",
                "view.palette",
                "view.sqllog",
                "view.overview",
            ]
        );
        for id in ids {
            assert!(ts.contains(&format!("\"{id}\"")), "{id} missing from MENU_IDS");
        }
        for (id, _) in super::APPEARANCE_ITEMS.into_iter().chain([super::TOUR_ITEM]) {
            assert!(ts.contains(&format!("\"{id}\"")), "{id} missing from MENU_IDS");
        }
    }

    #[test]
    fn editor_owned_keys_have_no_native_accelerator() {
        // ⌘F, ⌘[, ⌘], ⌘⇧C and ⌘⇧\ (CodeMirror's Shift-Mod-\) stay with the webview keymap.
        for id in ["view.filter", "view.sidebar", "view.inspector", "view.sqllog", "view.overview"] {
            assert_eq!(super::accelerator(id), None, "{id} must not steal an editor/webview key");
        }
        assert_eq!(super::accelerator("view.palette"), Some("CmdOrCtrl+K"));
        assert_eq!(super::accelerator("view.data"), Some("Alt+CmdOrCtrl+1"));
        assert_eq!(super::accelerator("view.structure"), Some("Alt+CmdOrCtrl+2"));
    }

    #[test]
    fn filter_item_is_labelled_filter_rows() {
        let label = super::view_items().into_iter().find(|(id, _)| *id == "view.filter").map(|(_, l)| l);
        assert_eq!(label, Some("Filter Rows…"));
    }
}
