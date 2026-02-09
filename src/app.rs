use crate::filesystem::{copy_entry, delete_entry, get_drives, read_directory, rename_entry, create_directory, create_file, FileEntry, FileType};
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::process::Command;
use serde::{Deserialize, Serialize};

const CONFIG_FILENAME: &str = "rust_explorer_config.json";

#[derive(Serialize, Deserialize)]
struct AppConfig {
    theme: Theme,
    favorites: Vec<PathBuf>,
    show_hidden: bool,
    sort_column: SortColumn,
    sort_order: SortOrder,
    last_path: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: Theme::Mocha,
            favorites: vec![
                std::env::current_dir().unwrap_or(PathBuf::from("C:\\")),
                dirs::home_dir().unwrap_or(PathBuf::from("C:\\Users")),
                dirs::desktop_dir().unwrap_or(PathBuf::from("C:\\Users\\Desktop")),
                dirs::document_dir().unwrap_or(PathBuf::from("C:\\Users\\Documents")),
                dirs::download_dir().unwrap_or(PathBuf::from("C:\\Users\\Downloads")),
            ],
            show_hidden: false,
            sort_column: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            last_path: std::env::current_dir().unwrap_or(PathBuf::from("C:\\")),
        }
    }
}

impl AppConfig {
    fn load() -> Self {
        if let Ok(content) = fs::read_to_string(CONFIG_FILENAME) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
        Self::default()
    }

    fn save(&self) {
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(CONFIG_FILENAME, content);
        }
    }
}

enum PreviewData {
    Text(String),
    Image(PathBuf),
}

#[derive(PartialEq, Serialize, Deserialize)]
enum SortColumn {
    Name,
    Size,
    Modified,
}

#[derive(PartialEq, Serialize, Deserialize)]
enum SortOrder {
    Ascending,
    Descending,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
enum Theme {
    Dark,
    Light,
    Mocha,
}

pub struct ExplorerApp {
    current_path: PathBuf,
    history: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,
    entries: Vec<FileEntry>,
    drives: Vec<PathBuf>,
    
    // Selection & State
    selected_entry: Option<usize>,
    preview_data: Option<PreviewData>,
    error_message: Option<String>,
    show_hidden: bool,
    theme: Theme,
    
    // Clipboard
    clipboard_path: Option<PathBuf>,
    
    // Renaming
    renaming_index: Option<usize>,
    rename_buffer: String,
    
    // Threading
    load_req_tx: Sender<PathBuf>,
    load_res_rx: Receiver<Result<Vec<FileEntry>, String>>,
    is_loading: bool,
    path_input: String,

    // New Features
    search_query: String,
    sort_column: SortColumn,
    sort_order: SortOrder,
    favorites: Vec<PathBuf>,
    creation_popup_open: bool,
    new_item_name: String,
    create_folder: bool, // true = folder, false = file
    path_edit_mode: bool,
}

impl ExplorerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&_cc.egui_ctx);
        let (tx, rx) = channel::<PathBuf>();
        let (res_tx, res_rx) = channel();

        thread::spawn(move || {
            while let Ok(path) = rx.recv() {
                let result = read_directory(&path);
                let _ = res_tx.send(result);
            }
        });

        let config = AppConfig::load();
        let start_path = if config.last_path.exists() { config.last_path.clone() } else { std::env::current_dir().unwrap_or(PathBuf::from("C:\\")) };
        
        tx.send(start_path.clone()).unwrap();

        let app = Self {
            current_path: start_path.clone(),
            history: Vec::new(),
            forward_stack: Vec::new(),
            entries: Vec::new(),
            drives: get_drives(),
            selected_entry: None,
            preview_data: None,
            error_message: None,
            show_hidden: config.show_hidden,
            theme: config.theme,
            clipboard_path: None,
            renaming_index: None,
            rename_buffer: String::new(),
            load_req_tx: tx,
            load_res_rx: res_rx,
            is_loading: true,
            path_input: start_path.to_string_lossy().to_string(),
            search_query: String::new(),
            sort_column: config.sort_column,
            sort_order: config.sort_order,
            favorites: config.favorites,
            creation_popup_open: false,
            new_item_name: String::new(),
            create_folder: true,
            path_edit_mode: false,
        };
        
        app.apply_theme(&_cc.egui_ctx);
        app
    }

    fn open_in_terminal(&mut self) {
        #[cfg(target_os = "windows")]
        let status = Command::new("powershell")
            .arg("-NoExit")
            .arg("-Command")
            .arg(format!("cd '{}'", self.current_path.to_string_lossy()))
            .spawn();
        
        #[cfg(not(target_os = "windows"))]
        let status = Command::new("sh").spawn(); // Placeholder for other OS

        if let Err(e) = status {
            self.error_message = Some(format!("Failed to open terminal: {}", e));
        }
    }


    fn save_state(&self) {
        let config = AppConfig {
            theme: self.theme,
            favorites: self.favorites.clone(),
            show_hidden: self.show_hidden,
            sort_column: match self.sort_column {
                SortColumn::Name => SortColumn::Name,
                SortColumn::Size => SortColumn::Size,
                SortColumn::Modified => SortColumn::Modified,
            }, // Cloning enum if Copy
            sort_order: match self.sort_order {
                SortOrder::Ascending => SortOrder::Ascending,
                SortOrder::Descending => SortOrder::Descending,
            },
            last_path: self.current_path.clone(),
        };
        config.save();
    }

    fn load_preview(&mut self) {
        self.preview_data = None;
        if let Some(idx) = self.selected_entry {
            if let Some(entry) = self.entries.get(idx) {
                if entry.file_type == FileType::File {
                    let ext = entry.path.extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_lowercase();
                    
                    match ext.as_str() {
                        "txt" | "rs" | "toml" | "md" | "json" | "js" | "ts" | "py" | "c" | "cpp" | "h" => {
                            if let Ok(content) = fs::read_to_string(&entry.path) {
                                // Limit preview size to 10KB
                                let preview = if content.len() > 10240 {
                                    format!("{}...", &content[..10240])
                                } else {
                                    content
                                };
                                self.preview_data = Some(PreviewData::Text(preview));
                            }
                        }
                        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tga" | "tiff" | "tif" | "pnm" | "dds" | "farbfeld" => {
                            self.preview_data = Some(PreviewData::Image(entry.path.clone()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn select_entry(&mut self, index: Option<usize>) {
        self.selected_entry = index;
        self.load_preview();
    }

    // --- Navigation ---

    fn navigate_to(&mut self, path: PathBuf, record_history: bool) {
        if record_history && self.current_path != path {
            self.history.push(self.current_path.clone());
            self.forward_stack.clear();
        }
        
        self.current_path = path.clone();
        self.path_input = path.to_string_lossy().to_string();
        self.is_loading = true;
        self.select_entry(None);
        self.renaming_index = None;
        self.error_message = None;
        let _ = self.load_req_tx.send(path);
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.forward_stack.push(self.current_path.clone());
            self.navigate_to(prev, false);
        }
    }

    fn go_forward(&mut self) {
        if let Some(next) = self.forward_stack.pop() {
            self.history.push(self.current_path.clone());
            self.navigate_to(next, false);
        }
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.navigate_to(parent.to_path_buf(), true);
        }
    }

    fn refresh(&mut self) {
        self.navigate_to(self.current_path.clone(), false);
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let visuals = match self.theme {
            Theme::Dark => egui::Visuals::dark(),
            Theme::Light => egui::Visuals::light(),
            Theme::Mocha => {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = egui::Color32::from_rgb(30, 30, 46); // Base
                visuals.window_fill = egui::Color32::from_rgb(30, 30, 46);
                visuals.extreme_bg_color = egui::Color32::from_rgb(24, 24, 37); // Mantle
                
                visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(30, 30, 46);
                visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(205, 214, 244)); // Text
                
                visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(49, 50, 68); // Surface0
                visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(69, 71, 90); // Surface1
                visuals.widgets.active.bg_fill = egui::Color32::from_rgb(88, 91, 112); // Surface2
                
                visuals.selection.bg_fill = egui::Color32::from_rgb(137, 180, 250).gamma_multiply(0.3); // Muted Blue
                visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(137, 180, 250));
                
                visuals.hyperlink_color = egui::Color32::from_rgb(137, 220, 235); // Sapphire
                visuals
            }
        };
        ctx.set_visuals(visuals);
    }

    fn sort_entries(&mut self) {
        self.entries.sort_by(|a, b| {
            let ordering = match self.sort_column {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Size => a.raw_size.cmp(&b.raw_size),
                SortColumn::Modified => a.modified.cmp(&b.modified),
            };
            
            if self.sort_order == SortOrder::Descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        
        // Always keep directories on top? or respect sort? 
        // Typically directories on top is preferred.
        self.entries.sort_by(|a, b| {
            match (a.file_type == FileType::Directory, b.file_type == FileType::Directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            }
        });
    }

    fn create_new_item(&mut self) {
        if self.new_item_name.is_empty() { return; }
        
        let result = if self.create_folder {
            create_directory(&self.current_path, &self.new_item_name)
        } else {
            create_file(&self.current_path, &self.new_item_name)
        };
        
        if let Err(e) = result {
            self.error_message = Some(format!("Creation failed: {}", e));
        } else {
            self.refresh();
        }
        self.creation_popup_open = false;
        self.new_item_name.clear();
    }

    // --- Operations ---

    fn open_entry(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index) {
            match entry.file_type {
                FileType::Directory => {
                    self.navigate_to(entry.path.clone(), true);
                }
                FileType::File | FileType::Symlink => {
                    if let Err(e) = open::that(&entry.path) {
                        self.error_message = Some(format!("Failed to open: {}", e));
                    }
                }
                FileType::Unknown => {}
            }
        }
    }

    fn start_rename(&mut self) {
        if let Some(idx) = self.selected_entry {
             if let Some(entry) = self.entries.get(idx) {
                 self.renaming_index = Some(idx);
                 self.rename_buffer = entry.name.clone();
             }
        }
    }

    fn confirm_rename(&mut self) {
        if let Some(idx) = self.renaming_index {
            if let Some(entry) = self.entries.get(idx) {
                if !self.rename_buffer.is_empty() && self.rename_buffer != entry.name {
                    if let Err(e) = rename_entry(&entry.path, &self.rename_buffer) {
                        self.error_message = Some(format!("Rename failed: {}", e));
                    } else {
                        self.refresh();
                    }
                }
            }
        }
        self.renaming_index = None;
    }

    fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_entry {
            if let Some(entry) = self.entries.get(idx) {
                if let Err(e) = delete_entry(&entry.path) {
                    self.error_message = Some(format!("Delete failed: {}", e));
                } else {
                    self.select_entry(None);
                    self.refresh();
                }
            }
        }
    }
    
    fn copy_selected(&mut self) {
        if let Some(idx) = self.selected_entry {
            if let Some(entry) = self.entries.get(idx) {
                self.clipboard_path = Some(entry.path.clone());
            }
        }
    }
    
    fn paste_clipboard(&mut self) {
        if let Some(src) = &self.clipboard_path {
            if let Err(e) = copy_entry(src, &self.current_path) {
                 self.error_message = Some(format!("Paste failed: {}", e));
            } else {
                self.refresh();
            }
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Event Handling ---
        if let Ok(result) = self.load_res_rx.try_recv() {
            self.is_loading = false;
            match result {
                Ok(mut entries) => {
                    if !self.show_hidden {
                        entries.retain(|e| !e.is_hidden);
                    }
                    self.entries = entries;
                    self.sort_entries();
                },
                Err(e) => self.error_message = Some(e),
            }
        }

        // Global Shortcuts
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
                self.go_up();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                self.start_rename();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete_selected();
            }
            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
                self.copy_selected();
            }
            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
                self.paste_clipboard();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
                self.refresh();
            }

            // Arrow key navigation
            if !self.entries.is_empty() {
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    let next = match self.selected_entry {
                        Some(idx) => (idx + 1).min(self.entries.len() - 1),
                        None => 0,
                    };
                    self.select_entry(Some(next));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    let next = match self.selected_entry {
                        Some(idx) => idx.saturating_sub(1),
                        None => 0,
                    };
                    self.select_entry(Some(next));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
                    self.select_entry(Some(0));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::End)) {
                    self.select_entry(Some(self.entries.len() - 1));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(idx) = self.selected_entry {
                        self.open_entry(idx);
                    }
                }
            }
        }

        // --- Top Navigation Bar ---
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("⬅").on_hover_text("Back").clicked() { self.go_back(); self.save_state(); }
                if ui.button("➡").on_hover_text("Forward").clicked() { self.go_forward(); self.save_state(); }
                if ui.button("⬆").on_hover_text("Up Level").clicked() { self.go_up(); self.save_state(); }
                if ui.button("⟳").on_hover_text("Refresh").clicked() { self.refresh(); }
                
                ui.separator();
                if ui.button("➕").on_hover_text("New Item").clicked() {
                    self.creation_popup_open = true;
                }
                
                ui.separator();
                if ui.checkbox(&mut self.show_hidden, "Hidden").changed() {
                    self.refresh();
                    self.save_state();
                }

                ui.separator();
                if ui.button("💻").on_hover_text("Open in Terminal").clicked() {
                    self.open_in_terminal();
                }

                ui.separator();
                let theme_changed = egui::ComboBox::from_label("")
                    .selected_text(match self.theme {
                        Theme::Dark => "🌙 Dark",
                        Theme::Light => "☀️ Light",
                        Theme::Mocha => "☕ Mocha",
                    })
                    .show_ui(ui, |ui| {
                        let mut changed = false;
                        if ui.selectable_value(&mut self.theme, Theme::Dark, "🌙 Dark").clicked() { changed = true; }
                        if ui.selectable_value(&mut self.theme, Theme::Light, "☀️ Light").clicked() { changed = true; }
                        if ui.selectable_value(&mut self.theme, Theme::Mocha, "☕ Mocha").clicked() { changed = true; }
                        changed
                    }).inner.unwrap_or(false);
                
                if theme_changed {
                    self.apply_theme(ctx);
                    self.save_state();
                }

                ui.add_space(10.0);
                
                // Breadcrumbs / Path Input
                ui.horizontal(|ui| {
                    if self.path_edit_mode {
                        let path_resp = ui.add_sized(
                            [300.0, ui.available_height()], 
                            egui::TextEdit::singleline(&mut self.path_input).hint_text("Path...")
                        );
                        
                        if path_resp.lost_focus() && path_resp.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let path = PathBuf::from(&self.path_input);
                            if path.exists() && path.is_dir() {
                                self.navigate_to(path, true);
                                self.save_state();
                            } else {
                                self.error_message = Some("Path not found".to_string());
                            }
                            self.path_edit_mode = false;
                        } else if path_resp.lost_focus() {
                            self.path_edit_mode = false;
                        }
                    } else {
                        // Breadcrumbs
                        let mut path_to_navigate = None;
                        egui::ScrollArea::horizontal().max_width(400.0).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let components: Vec<_> = self.current_path.iter().collect();
                                for (i, comp) in components.iter().enumerate() {
                                    let label = comp.to_string_lossy();
                                    let label = if label.is_empty() { "\\" } else { &label }; // Handle root better?
                                    if ui.button(label).clicked() {
                                        // Reconstruct path up to this component
                                        let mut new_path = PathBuf::new();
                                        for k in 0..=i {
                                            new_path.push(components[k]);
                                        }
                                        path_to_navigate = Some(new_path);
                                    }
                                    if i < components.len() - 1 {
                                        ui.label(">");
                                    }
                                }
                            });
                        });
                        
                        if let Some(p) = path_to_navigate {
                            self.navigate_to(p, true);
                            self.save_state();
                        }
                        
                        if ui.button("✏").on_hover_text("Edit Path").clicked() {
                            self.path_edit_mode = true;
                            self.path_input = self.current_path.to_string_lossy().to_string();
                        }
                    }
                });
                 
                 ui.add_space(10.0);
                 ui.label("🔍");
                 ui.add_sized(
                     ui.available_size(),
                     egui::TextEdit::singleline(&mut self.search_query).hint_text("Search...")
                 );
            });
            ui.add_space(4.0);
        });

        // --- Creation Popup ---
        if self.creation_popup_open {
            egui::Window::new("Create New")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.create_folder, true, "Folder");
                        ui.radio_value(&mut self.create_folder, false, "File");
                    });
                    ui.text_edit_singleline(&mut self.new_item_name);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            self.create_new_item();
                        }
                        if ui.button("Cancel").clicked() {
                            self.creation_popup_open = false;
                        }
                    });
                });
        }

        // --- Bottom Status Bar ---
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} items", self.entries.len()));
                if let Some(err) = &self.error_message {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, format!("⚠ {}", err));
                }
                
                // Show clipboard status
                if let Some(clip) = &self.clipboard_path {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("📋 Copied: {}", clip.file_name().unwrap_or_default().to_string_lossy())).italics());
                    });
                }
            });
        });

        // --- Side Panel (Drives & Favorites) ---
        egui::SidePanel::left("left_panel").resizable(true).default_width(150.0).show(ctx, |ui| {
            ui.heading("Favorites");
            ui.separator();
            let mut fav_to_open = None;
            egui::ScrollArea::vertical().id_salt("fav_scroll").max_height(150.0).show(ui, |ui| {
                for fav in &self.favorites {
                    let label = fav.file_name().unwrap_or(fav.as_os_str()).to_string_lossy().to_string();
                    let is_active = self.current_path == *fav;
                    if ui.selectable_label(is_active, &label).clicked() {
                         fav_to_open = Some(fav.clone());
                    }
                }
            });
            if let Some(path) = fav_to_open {
                self.navigate_to(path, true);
            }

            ui.separator();
            ui.heading("Drives");
            ui.separator();
            
            let mut drive_to_open = None;
            egui::ScrollArea::vertical().id_salt("drive_scroll").show(ui, |ui| {
                for drive in &self.drives {
                    let label = drive.to_string_lossy().to_string();
                    let is_active = self.current_path.starts_with(drive);
                    if ui.selectable_label(is_active, &label).clicked() {
                        drive_to_open = Some(drive.clone());
                    }
                }
            });
            if let Some(d) = drive_to_open { self.navigate_to(d, true); }
        });

        // --- Right Panel (Preview) ---
        if self.preview_data.is_some() {
            egui::SidePanel::right("right_panel").resizable(true).default_width(300.0).show(ctx, |ui| {
                ui.heading("Preview");
                ui.separator();

                match &self.preview_data {
                    Some(PreviewData::Text(content)) => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new(content).monospace());
                        });
                    }
                    Some(PreviewData::Image(path)) => {
                        let uri = format!("file://{}", path.to_string_lossy().replace("\\", "/"));
                        ui.add(egui::Image::new(uri).shrink_to_fit());
                    }
                    None => {}
                }
            });
        }

        // --- Main Content Area ---
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.is_loading {
                ui.centered_and_justified(|ui| { ui.spinner(); });
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("file_grid")
                        .striped(true)
                        .min_col_width(20.0)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            // Headers (Sortable)
                            if ui.button(egui::RichText::new("Name").strong()).clicked() {
                                if self.sort_column == SortColumn::Name {
                                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                                } else {
                                    self.sort_column = SortColumn::Name;
                                    self.sort_order = SortOrder::Ascending;
                                }
                                self.sort_entries();
                                self.save_state();
                            }
                            if ui.button(egui::RichText::new("Size").strong()).clicked() {
                                if self.sort_column == SortColumn::Size {
                                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                                } else {
                                    self.sort_column = SortColumn::Size;
                                    self.sort_order = SortOrder::Ascending;
                                }
                                self.sort_entries();
                                self.save_state();
                            }
                            if ui.button(egui::RichText::new("Modified").strong()).clicked() {
                                if self.sort_column == SortColumn::Modified {
                                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                                } else {
                                    self.sort_column = SortColumn::Modified;
                                    self.sort_order = SortOrder::Ascending;
                                }
                                self.sort_entries();
                                self.save_state();
                            }
                            ui.end_row();

                            // Rows
                            let mut action_to_perform = None; // (ActionType, Index)
                            let mut selection_to_make = None;
                            
                            // Filter entries based on search query
                            let filtered_indices: Vec<usize> = self.entries.iter().enumerate()
                                .filter(|(_, e)| self.search_query.is_empty() || e.name.to_lowercase().contains(&self.search_query.to_lowercase()))
                                .map(|(i, _)| i)
                                .collect();

                            for &i in &filtered_indices {
                                let entry = &self.entries[i];
                                let (icon, icon_color) = match entry.file_type {
                                    FileType::Directory => ("📁", egui::Color32::from_rgb(249, 226, 175)), // Yellow
                                    FileType::File => ("📄", egui::Color32::from_rgb(166, 173, 200)),      // Subtext1
                                    FileType::Symlink => ("🔗", egui::Color32::from_rgb(148, 226, 213)),   // Teal
                                    FileType::Unknown => ("?", egui::Color32::from_rgb(243, 139, 168)),    // Red
                                };

                                let is_selected = self.selected_entry == Some(i);
                                let is_renaming = self.renaming_index == Some(i);

                                // --- Name Column ---
                                if is_renaming {
                                    // Render input box
                                    let re = ui.text_edit_singleline(&mut self.rename_buffer);
                                    if re.lost_focus() || re.ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                                        action_to_perform = Some(("confirm_rename", i));
                                    }
                                    re.request_focus();
                                } else {
                                    // Render Icon and Label separately for better coloring
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.colored_label(icon_color, icon);
                                        
                                        let name_resp = ui.selectable_label(is_selected, &entry.name);
                                        
                                        if is_selected && ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::Home) || i.key_pressed(egui::Key::End)) {
                                            name_resp.scroll_to_me(None);
                                        }
                                        
                                        // Interactions
                                        if name_resp.clicked() {
                                            selection_to_make = Some(i);
                                            // Cancel renaming if clicking elsewhere
                                            if self.renaming_index.is_some() { self.renaming_index = None; }
                                        }
                                        if name_resp.double_clicked() {
                                            action_to_perform = Some(("open", i));
                                        }
                                        
                                        // Context Menu
                                        name_resp.context_menu(|ui| {
                                            if ui.button("Open").clicked() { action_to_perform = Some(("open", i)); ui.close_menu(); }
                                            ui.separator();
                                            if ui.button("Rename (F2)").clicked() { action_to_perform = Some(("rename", i)); ui.close_menu(); }
                                            if ui.button("Copy (Ctrl+C)").clicked() { action_to_perform = Some(("copy", i)); ui.close_menu(); }
                                            if ui.button("Delete (Del)").clicked() { action_to_perform = Some(("delete", i)); ui.close_menu(); }
                                        });
                                    });
                                }

                                // --- Metadata Columns (Dimmed) ---
                                let meta_color = egui::Color32::from_rgb(108, 112, 134); // Overlay0 (Grey)
                                ui.colored_label(meta_color, &entry.size);
                                ui.colored_label(meta_color, &entry.modified);
                                ui.end_row();
                            }

                            // Deferred actions to appease borrow checker
                            if let Some(idx) = selection_to_make {
                                self.select_entry(Some(idx));
                            }

                            if let Some((action, idx)) = action_to_perform {
                                match action {
                                    "open" => self.open_entry(idx),
                                    "rename" => {
                                        self.select_entry(Some(idx));
                                        self.start_rename();
                                    },
                                    "confirm_rename" => self.confirm_rename(),
                                    "copy" => {
                                        self.select_entry(Some(idx));
                                        self.copy_selected();
                                    },
                                    "delete" => {
                                        self.select_entry(Some(idx));
                                        self.delete_selected();
                                    },
                                    _ => {},
                                }
                            }
                        });
                });
            }
        });
    }
}
