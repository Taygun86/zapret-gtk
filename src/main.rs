use libadwaita as adw;
use gtk4 as gtk;

use adw::prelude::*;
use gtk::glib;

use adw::{Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationView, ToolbarView, ResponseAppearance};
use gtk::{Box, Orientation, Button, ProgressBar, Label, Entry, Spinner, ScrolledWindow, FileFilter, CheckButton};
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
            eprintln!("Çeviri kataloğu yüklenemedi.");
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
    init_i18n();

    let app = Application::builder()
        .application_id("com.ornek.zapret-gtk")
        .build();

    app.connect_activate(build_ui);

    app.run();
}

fn get_zapret_path() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| Path::new(".").to_path_buf())
        .join("zapret")
}

fn delete_local_zapret_folder() {
    let local_zapret = get_zapret_path();
    if local_zapret.exists() {
        println!("Yerel zapret klasörü siliniyor: {:?}", local_zapret);
        if let Err(_) = fs::remove_dir_all(&local_zapret) {
             let _ = Command::new("pkexec")
                .arg("rm")
                .arg("-rf")
                .arg(local_zapret)
                .output();
        }
    }
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
        .build();
    top_box1.append(&status_label);

    let placeholder_label = Label::builder()
        .label(&t("Bu uygulama, Zapret'in GTK arayüzü üzerinden kurulmasını ve yönetilmesini sağlayan bir uygulamadır")) 
        .margin_top(20)
        .margin_bottom(20)
        .wrap(true)
        .justify(gtk::Justification::Center)
        .visible(true)
        .build();
    top_box1.append(&placeholder_label);

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
        .build();
    top_box_test.append(&label_test_title);

    let label_test_info = Label::builder()
        .label(&t("Bu işlem internet hızınıza göre zaman alabilir.\nLütfen bekleyiniz."))
        .justify(gtk::Justification::Center)
        .margin_bottom(20)
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
        .label(&t("Listeyi doldurun (Enter tuşu yeni satır ekler):"))
        .margin_top(15)
        .margin_bottom(10)
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

    let add_button = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(&t("Yeni satır ekle"))
        .css_classes(vec!["flat"])
        .margin_bottom(10)
        .build();
    
    let entries_container_clone = entries_container.clone();
    add_button.connect_clicked(move |_| {
        add_entry_row(&entries_container_clone, true);
    });
    top_box2.append(&add_button);

    content_box2.append(&top_box2);

    let bottom_box2 = Box::new(Orientation::Vertical, 0);
    
    let action_buttons_box = Box::new(Orientation::Horizontal, 10);
    action_buttons_box.set_halign(gtk::Align::Center);
    action_buttons_box.set_margin_top(10);
    action_buttons_box.set_margin_bottom(10);

    let import_button = Button::builder()
        .icon_name("document-open-symbolic")
        .label(&t("İçe Aktar"))
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

    add_entry_row(&entries_container, false);

    let view2 = ToolbarView::builder()
        .content(&content_box2)
        .build();
    view2.add_top_bar(&header2);

    let page2 = NavigationPage::builder()
        .child(&view2)
        .title(&t("Zapret GTK"))
        .tag("settings_page")
        .build();


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
        .build();
    top_box_mgmt.append(&mgmt_title);

    let mgmt_desc = Label::builder()
        .label(&t("Aşağıda blockcheck testi sonucunda bulunan çalışan stratejiler listelenmiştir.\nKullanmak istediklerinizi seçip servisi yeniden başlatın."))
        .wrap(true)
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

    let strategies_list_box = Box::new(Orientation::Vertical, 0);
    strategies_list_box.add_css_class("boxed-list"); 
    scrolled_mgmt.set_child(Some(&strategies_list_box));
    top_box_mgmt.append(&scrolled_mgmt);

    content_box_mgmt.append(&top_box_mgmt);

    let bottom_box_mgmt = Box::new(Orientation::Vertical, 0);
    bottom_box_mgmt.set_margin_top(10);
    bottom_box_mgmt.set_margin_bottom(20);
    bottom_box_mgmt.set_margin_start(20);
    bottom_box_mgmt.set_margin_end(20);

    let mgmt_buttons_box = Box::new(Orientation::Horizontal, 10);
    mgmt_buttons_box.set_halign(gtk::Align::Center);

    let about_btn = Button::builder()
        .icon_name("help-about-symbolic")
        .css_classes(vec!["pill"])
        .tooltip_text(&t("Hakkında"))
        .build();
    mgmt_buttons_box.append(&about_btn);

    let export_button = Button::builder()
        .label(&t("Dışa Aktar"))
        .icon_name("document-save-symbolic")
        .css_classes(vec!["pill"])
        .build();
    mgmt_buttons_box.append(&export_button);

    let apply_button = Button::builder()
        .label(&t("Uygula ve Servisi Başlat"))
        .css_classes(vec!["suggested-action", "pill"])
        .build();
    mgmt_buttons_box.append(&apply_button);
    
    bottom_box_mgmt.append(&mgmt_buttons_box);
    content_box_mgmt.append(&bottom_box_mgmt);


    let view_mgmt = ToolbarView::builder()
        .content(&content_box_mgmt)
        .build();
    view_mgmt.add_top_bar(&header_mgmt);
    
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

    let status_label_mgmt = Label::builder()
        .label(&t("Kontrol ediliyor..."))
        .margin_start(10)
        .halign(gtk::Align::Start)
        .build();
    status_box.append(&status_label_mgmt);
    
    let service_buttons_box = Box::new(Orientation::Horizontal, 10);
    service_buttons_box.set_margin_bottom(10);
    service_buttons_box.set_margin_start(10);
    service_buttons_box.set_margin_end(10);
    
    let start_service_btn = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .label(&t("Başlat"))
        .build();

    let stop_service_btn = Button::builder()
        .icon_name("media-playback-stop-symbolic")
        .label(&t("Durdur"))
        .build();
        
    service_buttons_box.append(&start_service_btn);
    service_buttons_box.append(&stop_service_btn);
    
    
    status_box.append(&service_buttons_box);
    
    content_box_mgmt.append(&status_box);
    
    let status_label_mgmt_timer = status_label_mgmt.clone();
    
    glib::timeout_add_local(Duration::from_secs(10), move || {
        let output = Command::new("systemctl")
            .arg("is-active")
            .arg("zapret")
            .output();
            
        match output {
            Ok(o) => {
                let status_text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if status_text == "active" {
                    status_label_mgmt_timer.set_label(&t("Çalışıyor (Active)"));
                    status_label_mgmt_timer.add_css_class("success");
                    status_label_mgmt_timer.remove_css_class("error");
                } else {
                    status_label_mgmt_timer.set_label(&t("Durdu ({})").replace("{}", &status_text));
                    status_label_mgmt_timer.add_css_class("error");
                    status_label_mgmt_timer.remove_css_class("success");
                }
            },
            Err(_) => {
                status_label_mgmt_timer.set_label(&t("Servis durumu alınamadı"));
            }
        }
        glib::ControlFlow::Continue
    });
    


    let page_mgmt = NavigationPage::builder()
        .child(&view_mgmt)
        .title(&t("Zapret GTK"))
        .tag("management_page")
        .build();


    let window = ApplicationWindow::builder()
        .application(app)
        .title("Zapret GTK")
        .default_width(450)
        .default_height(500)
        .content(&nav_view)
        .build();

    start_service_btn.connect_clicked(move |_| {
         let _ = Command::new("pkexec").arg("systemctl").arg("start").arg("zapret").spawn();
    });
    
    stop_service_btn.connect_clicked(move |_| {
         let _ = Command::new("pkexec").arg("systemctl").arg("stop").arg("zapret").spawn();
    });
    
    let win_about = window.clone();
    about_btn.connect_clicked(move |_| {
        let bytes = glib::Bytes::from_static(ICON_BYTES);
        let texture = gdk::Texture::from_bytes(&bytes).expect("Icon load fail");

         let about = gtk::AboutDialog::builder()
            .transient_for(&win_about)
            .program_name("Zapret GTK")
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
    let window_clone = window.clone();
    let nav_view_clone = nav_view.clone();
    let window_clone_import = window.clone(); 
    
    let page_check_clone = page_check.clone();
    let page_test_clone = page_test.clone();

    let status_label_check_clone = status_label_check.clone();
    let conflict_list_label_clone = conflict_list_label.clone();
    let force_continue_button_clone = force_continue_button.clone();
    let spinner_check_clone = spinner_check.clone();
    let nav_view_clone_for_check = nav_view.clone();
    let page2_clone_for_check = page2.clone();
    
    let nav_view_clone_for_force = nav_view.clone();
    let page2_clone_for_force = page2.clone();

    let nav_view_clone_for_test = nav_view.clone();
    let label_test_counter_clone = label_test_counter.clone();

    let is_installation_complete = Rc::new(Cell::new(false));
    let is_complete_click = is_installation_complete.clone();
    let is_complete_done = is_installation_complete.clone();

    let is_installing = Rc::new(Cell::new(false));
    let is_installing_click = is_installing.clone();
    let is_installing_direct = is_installing.clone();

    let install_child_pid = Arc::new(Mutex::new(None::<u32>));
    let install_cancel_flag = Arc::new(AtomicBool::new(false)); 

    let install_child_pid_btn = install_child_pid.clone();
    let install_child_pid_run = install_child_pid.clone();
    let install_cancel_flag_btn = install_cancel_flag.clone();
    let install_cancel_flag_run = install_cancel_flag.clone();
    let install_cancel_flag_ui = install_cancel_flag.clone();

    let nav_view_clone_mgmt = nav_view.clone();
    let page_mgmt_clone = page_mgmt.clone();
    let list_box_mgmt = strategies_list_box.clone();

    let win_export = window.clone();
    export_button.connect_clicked(move |_| {
        if !Path::new("strategies.json").exists() {
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
            .initial_name("strategies.json")
            .modal(true)
            .accept_label(&t("Kaydet"))
            .build();
            
        let win_export_c = win_export.clone();
        
        file_dialog.save(Some(&win_export), None::<&gtk::gio::Cancellable>, move |result| {
             if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match fs::copy("strategies.json", &path) {
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

    let list_box_apply = strategies_list_box.clone();
    let win_apply = window.clone();
    
    apply_button.connect_clicked(move |_| {
        let mut selected_strategies = Vec::new();
        let mut child = list_box_apply.first_child();

        while let Some(widget) = child {
            if let Ok(check) = widget.clone().downcast::<CheckButton>() {
                if check.is_active() {
                   if let Some(label_txt) = check.label() {
                       selected_strategies.push(label_txt.to_string());
                   }
                }
            }
            child = widget.next_sibling();
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

        let combined_strategies = selected_strategies.join(" ");
        println!("Uygulanacak: {}", combined_strategies);

        let config_path = Path::new("/opt/zapret/config");
        let content_res = fs::read_to_string(config_path).or_else(|_| {
             let out = Command::new("pkexec").arg("cat").arg("/opt/zapret/config").output();
             match out {
                 Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
                 _ => Err(io::Error::new(io::ErrorKind::PermissionDenied, t("Dosya okunamadı"))),
             }
        });

        match content_res {
            Ok(content) => {
                let new_content = update_config_content(&content, &combined_strategies);
                
                let temp_path = "/tmp/zapret_config_new";
                if let Err(e) = fs::write(temp_path, &new_content) {
                     let dialog = adw::MessageDialog::builder()
                        .transient_for(&win_apply)
                        .heading(&t("Hata"))
                        .body(&t("Geçici dosya oluşturulamadı: {}").replace("{}", &e.to_string()))
                        .build();
                    dialog.add_response("ok", &t("Tamam"));
                    dialog.present();
                    return;
                }

                let cmd_script = format!("mv -f {} /opt/zapret/config && systemctl restart zapret", temp_path);
                
                let res = Command::new("pkexec")
                    .arg("sh")
                    .arg("-c")
                    .arg(cmd_script)
                    .output();

                match res {
                    Ok(output) if output.status.success() => {
                        let dialog = adw::MessageDialog::builder()
                            .transient_for(&win_apply)
                            .heading(&t("Başarılı"))
                            .body(&t("Stratejiler config dosyasına yazıldı ve Zapret servisi yeniden başlatıldı."))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                    },
                    Ok(output) => {
                         let err = String::from_utf8_lossy(&output.stderr);
                         let dialog = adw::MessageDialog::builder()
                            .transient_for(&win_apply)
                            .heading(&t("Hata"))
                            .body(&t("Servis başlatılamadı:\n{}").replace("{}", &err))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                    },
                    Err(e) => {
                         let dialog = adw::MessageDialog::builder()
                            .transient_for(&win_apply)
                            .heading(&t("Hata"))
                            .body(&t("Komut hatası: {}").replace("{}", &e.to_string()))
                            .build();
                        dialog.add_response("ok", &t("Tamam"));
                        dialog.present();
                    }
                }
            },
            Err(e) => {
                 let dialog = adw::MessageDialog::builder()
                    .transient_for(&win_apply)
                    .heading(&t("Okuma Hatası"))
                    .body(&t("Config dosyası okunamadı: {}").replace("{}", &e.to_string()))
                    .build();
                dialog.add_response("ok", &t("Tamam"));
                dialog.present();
            }
        }
    });

    if Path::new("/opt/zapret").exists() && Path::new("strategies.json").exists() {
        delete_local_zapret_folder();

         if let Ok(content) = fs::read_to_string("strategies.json") {
             let trimmed = content.trim();
             if trimmed.starts_with('[') {
                let inner = &trimmed[1..trimmed.len()-1];
                let mut in_string = false;
                let mut current_strat = String::new();
                let mut strategies = Vec::new();
                
                for c in inner.chars() {
                    if c == '"' {
                        in_string = !in_string;
                        if !in_string && !current_strat.is_empty() {
                            strategies.push(current_strat.clone());
                            current_strat.clear();
                        }
                    } else if in_string {
                        if c != '\\' { 
                            current_strat.push(c); 
                        }
                    }
                }
                
                for strat in strategies {
                    let check = CheckButton::builder()
                        .label(&strat)
                        .margin_top(10)
                        .margin_bottom(10)
                        .margin_start(10)
                        .margin_end(10)
                        .build();
                    strategies_list_box.append(&check);
                }
                
                nav_view.push(&page_mgmt);
             }
         }
    }

    button.connect_clicked(move |_| {
        if is_installing_click.get() {
            install_cancel_flag_btn.store(true, Ordering::Relaxed);

            if let Ok(guard) = install_child_pid_btn.lock() {
                if let Some(pid) = *guard {
                    let _ = Command::new("kill")
                        .arg("-9")
                        .arg(pid.to_string())
                        .spawn();
                }
            }

            is_installing_click.set(false);
            
            button_clone.set_label(&t("Kuruluma Başla"));
            button_clone.remove_css_class("destructive-action");
            button_clone.remove_css_class("warning");
            button_clone.remove_css_class("error");
            button_clone.add_css_class("suggested-action");
            button_clone.set_sensitive(true);

            placeholder_label_clone.set_visible(true);
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

        let zapret_path = get_zapret_path();
        if zapret_path.exists() {
            let dialog = adw::MessageDialog::builder()
                .transient_for(&window_clone)
                .modal(true)
                .heading(&t("Klasör Bulundu"))
                .body(&t("Mevcut bir 'zapret' klasörü tespit edildi. Ne yapmak istersiniz?"))
                .build();

            dialog.add_response("cancel", &t("İptal"));
            dialog.add_response("accept", &t("Mevcut Olanı Kullan"));
            dialog.add_response("reject", &t("Sil ve İndir"));

            dialog.set_response_appearance("reject", ResponseAppearance::Destructive);
            dialog.set_response_appearance("accept", ResponseAppearance::Suggested);

            let btn_c = button_clone.clone();
            let pb_c = progress_bar_clone.clone();
            let lbl_c = status_label_clone.clone();
            let pl_c = placeholder_label_clone.clone();
            let is_comp = is_complete_done.clone();
            let is_inst = is_installing_direct.clone();
            let pid_store = install_child_pid_run.clone();
            let cancel_flg = install_cancel_flag_run.clone();
            let cancel_flg_ui = install_cancel_flag_ui.clone();

            dialog.connect_response(None, move |d, response_id| {
                match response_id {
                    "reject" => {
                        d.close();
                        run_installation(btn_c.clone(), pb_c.clone(), lbl_c.clone(), pl_c.clone(), true, is_comp.clone(), is_inst.clone(), pid_store.clone(), cancel_flg.clone(), cancel_flg_ui.clone());
                    },
                    "accept" => {
                        d.close();
                        run_installation(btn_c.clone(), pb_c.clone(), lbl_c.clone(), pl_c.clone(), false, is_comp.clone(), is_inst.clone(), pid_store.clone(), cancel_flg.clone(), cancel_flg_ui.clone());
                    },
                    _ => {
                        d.close();
                    }
                }
            });

            dialog.present();
        } else {
            run_installation(button_clone.clone(), progress_bar_clone.clone(), status_label_clone.clone(), placeholder_label_clone.clone(), false, is_complete_done.clone(), is_installing_direct.clone(), install_child_pid_run.clone(), install_cancel_flag_run.clone(), install_cancel_flag_ui.clone());
        }
    });

    force_continue_button.connect_clicked(move |_| {
        nav_view_clone_for_force.replace(&[page2_clone_for_force.clone()]);
    });

    let current_pid = Arc::new(Mutex::new(None::<u32>));
    let current_pid_cancel = current_pid.clone();
    let nav_view_clone_cancel = nav_view.clone();

    let test_cancel_flag = Arc::new(AtomicBool::new(false));
    let test_cancel_flag_btn = test_cancel_flag.clone();
    let _test_cancel_flag_run = test_cancel_flag.clone();

    test_cancel_button.connect_clicked(move |_| {
        test_cancel_flag_btn.store(true, Ordering::Relaxed);

        if let Ok(guard) = current_pid_cancel.lock() {
            if let Some(pid) = *guard {
                println!("İşlem iptal ediliyor... PID: {}", pid);
                let _ = Command::new("pkexec")
                    .arg("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .spawn();
            }
        }
        nav_view_clone_cancel.pop();
    });

    let nav_view_clone_import_btn = nav_view.clone();
    let page_test_clone_import = page_test.clone();
    let lbl_test_clone_import = label_test_counter.clone();
    let pid_clone_import = current_pid.clone();
    let cf_clone_import = test_cancel_flag.clone();
    let nav_mgmt_import = nav_view_clone_mgmt.clone();
    let page_mgmt_import = page_mgmt_clone.clone();
    let list_mgmt_import = list_box_mgmt.clone();

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
        let pid = pid_clone_import.clone();
        let cf = cf_clone_import.clone();
        
        let list_box_mgmt_import_timer = list_mgmt_import.clone();
        let nav_mgmt_import_timer = nav_mgmt_import.clone();
        let page_mgmt_import_timer = page_mgmt_import.clone();

        file_dialog.open(Some(&window_clone_import), None::<&gtk::gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match validate_and_copy_strategies(&path) {
                        Ok(_) => {
                            cf.store(false, Ordering::Relaxed);
                            nav.push(&page);
                            lbl.set_label(&t("Zapret Kuruluyor (/opt/zapret)..."));
                            
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
                                                        let mut child = list_box_mgmt_import_timer.first_child();
                                                        while let Some(widget) = child {
                                                            let next = widget.next_sibling();
                                                            list_box_mgmt_import_timer.remove(&widget);
                                                            child = next;
                                                        }

                                                        if let Ok(content) = fs::read_to_string("strategies.json") {
                                                            let trimmed = content.trim();
                                                            if trimmed.starts_with('[') {
                                                                let inner = &trimmed[1..trimmed.len()-1];
                                                                let mut in_string = false;
                                                                let mut current_strat = String::new();
                                                                let mut strategies = Vec::new();
                                                                
                                                                for c in inner.chars() {
                                                                    if c == '"' {
                                                                        in_string = !in_string;
                                                                        if !in_string && !current_strat.is_empty() {
                                                                            strategies.push(current_strat.clone());
                                                                            current_strat.clear();
                                                                        }
                                                                    } else if in_string {
                                                                        if c != '\\' { 
                                                                             current_strat.push(c); 
                                                                        }
                                                                    }
                                                                }
                                                                
                                                                for strat in strategies {
                                                                    let check = CheckButton::builder()
                                                                        .label(&strat)
                                                                        .margin_top(10)
                                                                        .margin_bottom(10)
                                                                        .margin_start(10)
                                                                        .margin_end(10)
                                                                        .build();
                                                                    list_box_mgmt_import_timer.append(&check);
                                                                }
                                                            }
                                                        }
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
        let pid = current_pid.clone(); 
        let win = window_clone_msg.clone();
        let d_list = domains.clone();
        
        let nav_mgmt = nav_view_clone_mgmt.clone();
        let page_mgmt = page_mgmt_clone.clone();
        let list_mgmt = list_box_mgmt.clone();

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

            nav.push(&page);
            lbl.set_label(&t("Denenen Stratejiler: 0"));

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
                                        if let Err(e) = save_strategies_to_json(&strategies) {
                                            let dialog = adw::MessageDialog::builder()
                                                .transient_for(&win_timer)
                                                .heading(&t("Kaydetme Hatası"))
                                                .body(&t("Dosya kaydedilemedi: {}").replace("{}", &e.to_string()))
                                                .build();
                                            dialog.add_response("ok", &t("Tamam"));
                                            dialog.present();
                                            glib::ControlFlow::Break
                                        } else {
                                            lbl_timer.set_label(&t("Zapret Kuruluyor (/opt/zapret)..."));
                                            
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
                                        let mut child = list_box_mgmt_timer.first_child();
                                        while let Some(widget) = child {
                                            let next = widget.next_sibling();
                                            list_box_mgmt_timer.remove(&widget);
                                            child = next;
                                        }

                                        if let Ok(content) = fs::read_to_string("strategies.json") {
                                            let trimmed = content.trim();
                                            if trimmed.starts_with('[') {
                                                let inner = &trimmed[1..trimmed.len()-1];
                                                let mut in_string = false;
                                                let mut current_strat = String::new();
                                                let mut strategies = Vec::new();
                                                
                                                for c in inner.chars() {
                                                    if c == '"' {
                                                        in_string = !in_string;
                                                        if !in_string && !current_strat.is_empty() {
                                                            strategies.push(current_strat.clone());
                                                            current_strat.clear();
                                                        }
                                                    } else if in_string {
                                                        if c != '\\' { 
                                                            current_strat.push(c); 
                                                        }
                                                    }
                                                }
                                                
                                                for strat in strategies {
                                                    let check = CheckButton::builder()
                                                        .label(&strat)
                                                        .margin_top(10)
                                                        .margin_bottom(10)
                                                        .margin_start(10)
                                                        .margin_end(10)
                                                        .build();
                                                    list_box_mgmt_timer.append(&check);
                                                }
                                            }
                                        }

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

fn validate_and_copy_strategies(path: &Path) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let trimmed = content.trim();

    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, t("Dosya geçerli bir JSON listesi (array) formatında değil.")));
    }

    let mut in_string = false;
    let mut escaped = false;
    let mut current_string = String::new();
    let mut strategies = Vec::new();

    for c in trimmed[1..trimmed.len()-1].chars() {
        if in_string {
            if escaped {
                current_string.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
                strategies.push(current_string.clone());
                current_string.clear();
            } else {
                current_string.push(c);
            }
        } else {
            if c == '"' {
                in_string = true;
            }
        }
    }

    if strategies.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, t("Dosya içerisinde strateji bulunamadı.")));
    }

    for s in strategies {
        if !s.trim().starts_with("--") {
            return Err(io::Error::new(io::ErrorKind::InvalidData, t("Geçersiz strateji: '{}'. Stratejiler '--' ile başlamalıdır.").replace("{}", &s)));
        }
    }

    let dest = Path::new("strategies.json");
    fs::write(dest, content)?;
    Ok(())
}

fn update_config_content(content: &str, new_opt: &str) -> String {
    let var_name = "NFQWS_OPT=\"";
    
    if let Some(start_idx) = content.find(var_name) {
        let content_after_start = &content[start_idx + var_name.len()..];
        
        let mut end_offset = 0;
        let mut escaped = false;
        let mut found = false;
        
        for (i, c) in content_after_start.char_indices() {
            if escaped {
                escaped = false;
            } else {
                if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    end_offset = i;
                    found = true;
                    break;
                }
            }
        }
        
        if found {
            let prefix = &content[..start_idx];
            let suffix = &content_after_start[end_offset + 1..];
            return format!("{}NFQWS_OPT=\"{}\"{}", prefix, new_opt, suffix);
        }
    }
    
    let var_name_single = "NFQWS_OPT='";
    if let Some(start_idx) = content.find(var_name_single) {
        let content_after_start = &content[start_idx + var_name_single.len()..];
         if let Some(end_offset) = content_after_start.find('\'') {
             let prefix = &content[..start_idx];
             let suffix = &content_after_start[end_offset + 1..];
             return format!("{}NFQWS_OPT=\"{}\"{}", prefix, new_opt, suffix);
         }
    }
    
    format!("{}\nNFQWS_OPT=\"{}\"\n", content, new_opt)
}

fn run_blockcheck_process(domains: Vec<String>, repeats: usize, scan_level: String, sender: mpsc::Sender<TestMsg>, cancel_flag: Arc<AtomicBool>) {
    let domains_str = domains.join(" ");
    
    let zapret_dir = get_zapret_path();
    let blockcheck_script = zapret_dir.join("blockcheck.sh");
    
    if !blockcheck_script.exists() {
        let _ = sender.send(TestMsg::Finished(Err(io::Error::new(io::ErrorKind::NotFound, t("blockcheck.sh bulunamadı: {}").replace("{}", &blockcheck_script.display().to_string())))));
        return;
    }

    let zapret_base_str = zapret_dir.to_string_lossy().to_string();

    println!("Executing blockcheck: pkexec env ... {:?}", blockcheck_script);

    let mut child = match Command::new("pkexec")
        .arg("env")
        .arg("BATCH=1")
        .arg(format!("REPEATS={}", repeats))
        .arg(format!("SCANLEVEL={}", scan_level))
        .arg("SKIP_TPWS=1")
        .arg("ENABLE_HTTP=1")
        .arg("ENABLE_HTTPS_TLS12=1")
        .arg("ENABLE_HTTPS_TLS13=1")
        .arg(format!("ZAPRET_BASE={}", zapret_base_str))
        .arg(format!("DOMAINS={}", domains_str))
        .arg(blockcheck_script)
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
                println!("Thread: İptal bayrağı algılandı, işlem durduruluyor.");
                let _ = child.kill();
                let _ = child.wait(); 
                return; 
            }

            match line_result {
                Ok(line) => {
                    println!("{}", line);
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
                        strategies.push(strategy);
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
                        if !strategies.contains(&strategy) {
                            strategies.push(strategy);
                        }
                     }
                 }
             }
        }
        
        let _ = sender.send(TestMsg::Finished(Ok(strategies)));

    } else {
        let _ = sender.send(TestMsg::Finished(Err(io::Error::new(io::ErrorKind::Other, t("Stdout alınamadı.")))));
    }
}

fn run_easy_install_script(sender: mpsc::Sender<TestMsg>, cancel_flag: Arc<AtomicBool>) {
    let zapret_dir = get_zapret_path();
    let install_script = zapret_dir.join("install_easy.sh");
    
    if !install_script.exists() {
        let _ = sender.send(TestMsg::InstallFinished(Err(io::Error::new(io::ErrorKind::NotFound, t("install_easy.sh bulunamadı")))));
        return;
    }

    let inputs = "Y\nY\nN\n1\nN\nN\nY\nN\n\n\n";
    let input_path = Path::new("/tmp/zapret_install_inputs.txt");
    if let Err(e) = fs::write(input_path, inputs) {
         let _ = sender.send(TestMsg::InstallFinished(Err(e)));
         return;
    }

    let zapret_base_str = zapret_dir.to_string_lossy().to_string();

    let wrapper_content = format!(
        "#!/bin/sh\nexport ZAPRET_BASE=\"{}\"\n\"{}\" < \"{}\"\n", 
        zapret_base_str, 
        install_script.to_string_lossy(), 
        input_path.to_string_lossy()
    );
    let wrapper_path = Path::new("/tmp/zapret_wrapper_run.sh");
    if let Err(e) = fs::write(wrapper_path, wrapper_content) {
        let _ = sender.send(TestMsg::InstallFinished(Err(e)));
        return;
    }
    let _ = Command::new("chmod").arg("+x").arg(wrapper_path).output();

    let wrapper_content_fixed = format!(
        "#!/bin/sh\nexport ZAPRET_BASE=\"{}\"\n\"{}\" < \"{}\"\nexit_code=$?\nif [ $exit_code -eq 0 ]; then\nsed -i 's/^NFQWS_ENABLE=.*/NFQWS_ENABLE=1/' /opt/zapret/config\nfi\nexit $exit_code\n", 
        zapret_base_str, 
        install_script.to_string_lossy(), 
        input_path.to_string_lossy()
    );
    if let Err(e) = fs::write(wrapper_path, wrapper_content_fixed) {
        let _ = sender.send(TestMsg::InstallFinished(Err(e)));
        return;
    }

    let mut child = match Command::new("pkexec")
        .arg(wrapper_path)
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
                let _ = child.kill();
                return;
            }
            if let Ok(line) = line_result {
                println!("[INSTALL]: {}", line);
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

fn save_strategies_to_json(strategies: &Vec<String>) -> io::Result<()> {
    let mut file = fs::File::create("strategies.json")?;
    writeln!(file, "[")?;
    for (i, s) in strategies.iter().enumerate() {
        let escaped = s.replace("\"", "\\\"");
        write!(file, "  \"{}\"", escaped)?;
        if i < strategies.len() - 1 {
            writeln!(file, ",")?;
        } else {
            writeln!(file, "")?;
        }
    }
    writeln!(file, "]")?;
    Ok(())
}

fn add_entry_row(container: &Box, grab_focus: bool) {
    let entry = Entry::builder()
        .placeholder_text("Veri girin...")
        .build();
    
    let container_clone = container.clone();
    
    entry.connect_activate(move |_| {
        add_entry_row(&container_clone, true);
    });

    container.append(&entry);

    if grab_focus {
        entry.grab_focus();
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
        "zapret"
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

fn run_installation(btn: Button, pb: ProgressBar, lbl: Label, placeholder: Label, overwrite: bool, is_complete_flag: Rc<Cell<bool>>, is_installing_flag: Rc<Cell<bool>>, pid_store: Arc<Mutex<Option<u32>>>, cancel_flag: Arc<AtomicBool>, cancel_flag_ui: Arc<AtomicBool>) {
    is_installing_flag.set(true);
    cancel_flag.store(false, Ordering::Relaxed);

    btn.set_sensitive(false);
    
    placeholder.set_visible(false);
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

        let mut root_commands = String::from("#!/bin/sh\nset -e\nexec 2>&1\n");
        let mut needs_root_permission = false;
        
        let distro_id = get_distro_id();
        
        let zapret_full_path = get_zapret_path();
        let zapret_path_str = zapret_full_path.to_string_lossy().to_string();

        if cancel_flag_thread.load(Ordering::Relaxed) { return; }

        if overwrite && zapret_full_path.exists() {
            root_commands.push_str("echo \"STATUS:CLEANING\"\n");
            root_commands.push_str(&format!("rm -rf \"{}\"\n", zapret_path_str));
            needs_root_permission = true;
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
             let install_parts = get_package_install_command(&distro_id, lib);
             if !install_parts.is_empty() {
                 dep_install_commands.push(install_parts.join(" "));
             }
        }

        if !dep_install_commands.is_empty() {
            root_commands.push_str("echo \"STATUS:INSTALLING_DEPS\"\n");
            
            match distro_id.as_str() {
                "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => {
                    root_commands.push_str("apt-get update\n");
                },
                "arch" | "manjaro" | "endeavouros" | "cachyos" => {
                    root_commands.push_str("pacman -Sy\n");
                },
                "fedora" => {
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
            if !needs_root_permission { needs_root_permission = true; }
        }

        if cancel_flag_thread.load(Ordering::Relaxed) { return; }

        root_commands.push_str("echo \"STATUS:CONFIGURING\"\n");
        
        let config_file = "/etc/dnscrypt-proxy/dnscrypt-proxy.toml";
        
        root_commands.push_str(&format!("if [ -f \"{}\" ]; then\n", config_file));
        root_commands.push_str(&format!("  sed -i \"40s/^listen_addresses = \\['127\\.0\\.0\\.1:53'\\]$/listen_addresses = ['127.0.0.1:53', '[::1]:53']/\" {}\n", config_file));
        root_commands.push_str("fi\n");
        if !needs_root_permission { needs_root_permission = true; }


        {
            root_commands.push_str("echo \"STATUS:FINALIZING\"\n");

            root_commands.push_str("systemctl restart NetworkManager\n");
            root_commands.push_str("systemctl enable dnscrypt-proxy.service\n");
            root_commands.push_str("systemctl start dnscrypt-proxy.service\n");
            
            if !needs_root_permission { needs_root_permission = true; }
        }

        if cancel_flag_thread.load(Ordering::Relaxed) { return; }

        if needs_root_permission {
            let _ = sender.send(AppMsg::Status(t("Yetki onayı bekleniyor...")));
            
            let script_path = "/tmp/zapret_installer_job.sh";
            if let Ok(mut file) = fs::File::create(script_path) {
                let _ = file.write_all(root_commands.as_bytes());
            }

            println!("--- Installer Script Content ---\n{}\n--------------------------------", root_commands);

            let mut child = Command::new("pkexec")
                .arg("/bin/sh")
                .arg(script_path)
                .stdout(Stdio::piped())
                .spawn()
                .expect("pkexec başlatılamadı");

            let _ = sender.send(AppMsg::PID(child.id()));

            let mut last_error_line = String::new();

            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if cancel_flag_thread.load(Ordering::Relaxed) { break; }

                    if let Ok(l) = line {
                        println!("[Installer]: {}", l);
                        
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
                        } else if l.contains("STATUS:FINALIZING") {
                            let _ = sender.send(AppMsg::Status(t("Ağ ayarları ve servisler başlatılıyor...")));
                        }
                    }
                }
            }

            if cancel_flag_thread.load(Ordering::Relaxed) {
                return; 
            }

            let status = child.wait();

            match status {
                Ok(s) if s.success() => {
                    let _ = sender.send(AppMsg::Status(t("NetworkManager Bekleniyor...")));
                    let _ = fs::remove_file(script_path);
                    
                    thread::sleep(Duration::from_secs(5));
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

        if cancel_flag_thread.load(Ordering::Relaxed) { return; }

        if !zapret_full_path.exists() {
            let _ = sender.send(AppMsg::Status(t("Zapret deposu indiriliyor...")));
            
            let git_result = Command::new("git")
                .args(["clone", "https://github.com/bol-van/zapret.git", zapret_path_str.as_str()])
                .status();

            match git_result {
                Ok(s) if s.success() => {
                    if cancel_flag_thread.load(Ordering::Relaxed) { return; }

                    let _ = sender.send(AppMsg::Status(t("Zapret derleniyor (make)...")));
                    
                    let mut make_cmd = Command::new("make");
                    make_cmd.arg("-C").arg(&zapret_full_path);
                    make_cmd.stdout(Stdio::piped());
                    make_cmd.stderr(Stdio::piped());
                    
                    if let Ok(mut child) = make_cmd.spawn() {
                         let _ = sender.send(AppMsg::PID(child.id()));
                         
                         let mut make_last_error = String::new();
                         if let Some(stdout) = child.stdout.take() {
                            let reader = BufReader::new(stdout);
                            for line in reader.lines() {
                                if let Ok(l) = line { 
                                    println!("[MAKE_OUT]: {}", l); 
                                    make_last_error = l;
                                }
                            }
                         }
                         if let Some(stderr) = child.stderr.take() {
                            let reader = BufReader::new(stderr);
                            for line in reader.lines() {
                                if let Ok(l) = line {
                                     println!("[MAKE_ERR]: {}", l);
                                     make_last_error = l;
                                }
                            }
                         }
                         
                         let make_result = child.wait();
                         
                         if cancel_flag_thread.load(Ordering::Relaxed) { return; }

                         match make_result {
                            Ok(m) if m.success() => {
                                 let _ = sender.send(AppMsg::Done(Ok(())));
                            },
                            Ok(m) => {
                                 let _ = sender.send(AppMsg::Done(Err(io::Error::new(io::ErrorKind::Other, t("Make hatası ({c}): {e}").replace("{c}", &m.code().unwrap_or(-1).to_string()).replace("{e}", &make_last_error)))));
                            },
                            Err(e) => {
                                 let _ = sender.send(AppMsg::Done(Err(e)));
                            }
                        }
                    } else {
                         let _ = sender.send(AppMsg::Done(Err(io::Error::new(io::ErrorKind::Other, t("Make komutu başlatılamadı. 'make' kurulu mu?")))));
                    }
                },
                Ok(_) => {
                    let _ = sender.send(AppMsg::Done(Err(io::Error::new(io::ErrorKind::Other, t("Git clone hatası.")))));
                },
                Err(e) => {
                    let _ = sender.send(AppMsg::Done(Err(e)));
                }
            }
        } else {
             let _ = sender.send(AppMsg::Status(t("Mevcut zapret klasörü kullanılıyor.")));
             thread::sleep(Duration::from_millis(500));
             if cancel_flag_thread.load(Ordering::Relaxed) { return; }
             let _ = sender.send(AppMsg::Done(Ok(())));
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
                                btn.set_label(&t("Devam"));
                                btn.remove_css_class("destructive-action");
                                btn.add_css_class("success");
                                lbl.set_label(&t("Kurulum bitti. Devam edebilirsiniz."));
                                is_complete_flag.set(true);
                            },
                            Err(e) => {
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

fn get_package_install_command(distro: &str, package: &str) -> Vec<String> {
    let mut p = package.to_string();
    
    if package == "gcc" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => p = "build-essential".to_string(),
            "alpine" => p = "build-base".to_string(),
            _ => p = "gcc".to_string(),
        }
    } else if package == "zlib" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => p = "zlib1g-dev".to_string(),
            "fedora" => p = "zlib-devel".to_string(),
            "alpine" => p = "zlib-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" => p = "zlib".to_string(),
            _ => p = "zlib-devel".to_string(),
        }
    } else if package == "libnetfilter_queue" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => p = "libnetfilter-queue-dev libnfnetlink-dev".to_string(),
            "fedora" => p = "libnetfilter_queue-devel libnfnetlink-devel".to_string(),
            "alpine" => p = "libnetfilter_queue-dev libnfnetlink-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" => p = "libnetfilter_queue libnfnetlink".to_string(),
            _ => p = "libnetfilter_queue-devel".to_string(),
        }
    } else if package == "libmnl" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => p = "libmnl-dev".to_string(),
            "fedora" => p = "libmnl-devel".to_string(),
            "alpine" => p = "libmnl-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" => p = "libmnl".to_string(),
            _ => p = "libmnl-devel".to_string(),
        }
    } else if package == "libcap" {
        match distro {
            "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => p = "libcap-dev".to_string(),
            "fedora" => p = "libcap-devel".to_string(),
            "alpine" => p = "libcap-dev".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" => p = "libcap".to_string(),
            _ => p = "libcap-devel".to_string(),
        }
    } else if package == "dig" {
        match distro {
            "void" | "fedora" => p = "bind-utils".to_string(),
            "alpine" => p = "bind-tools".to_string(),
            "arch" | "manjaro" | "endeavouros" | "cachyos" => p = "bind".to_string(),
            _ => p = "dnsutils".to_string(),
        }
    }

    match distro {
        "arch" | "manjaro" | "endeavouros" | "cachyos" => vec!["pacman".to_string(), "-S".to_string(), "--noconfirm".to_string(), p],
        "fedora" => vec!["dnf".to_string(), "install".to_string(), "-y".to_string(), p],
        "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" => vec!["zypper".to_string(), "--non-interactive".to_string(), "in".to_string(), p],
        "alpine" => vec!["apk".to_string(), "add".to_string(), p],
        "void" => vec!["xbps-install".to_string(), "-S".to_string(), "-y".to_string(), p],
        "gentoo" => vec!["emerge".to_string(), p],
        "ubuntu" | "debian" | "linuxmint" | "pop" | "kali" => vec!["apt-get".to_string(), "install".to_string(), "-y".to_string(), p],
        _ => vec![],
    }
}