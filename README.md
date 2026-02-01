# Zapret GTK

**[English](#english) | [Türkçe](#türkçe) | [Русский](#русский)**

---

## English
**Zapret GTK** is a modern GTK4 graphical interface for the [zapret](https://github.com/bol-van/zapret) DPI bypass tool on Linux. It simplifies the process of finding working strategies and managing the background service.

### Features
*   **Easy Installation:** Downloads and installs zapret automatically.
*   **Blockcheck GUI:** Graphical wizard to run `blockcheck` and find working strategies against DPI.
*   **Strategy Management:** Select and apply multiple strategies easily.
*   **Service Control:** Start, stop, and monitor the `zapret` system service.
*   **Portable:** Single binary with no external resource dependencies.

### Installation

#### Debian / Ubuntu / Linux Mint
Add the repository and install the package:
```bash
echo "deb [trusted=yes] https://taygun86.github.io/taygun86-repo/deb ./" | sudo tee /etc/apt/sources.list.d/taygun86.list
sudo apt update
sudo apt install zapret-gtk
```

#### Fedora / OpenSUSE / RHEL
Add the repository and install the package:
```bash
sudo tee /etc/yum.repos.d/taygun86.repo <<EOF
[taygun86]
name=Taygun86 Repository
baseurl=https://taygun86.github.io/taygun86-repo/rpm
enabled=1
gpgcheck=0
EOF

sudo dnf install zapret-gtk
```

#### Arch Linux / Manjaro / CachyOS / EndeavourOS
Install from AUR:
```bash
yay -S zapret-gtk
```

### Build & Run (Manual)
Requirements: `libgtk-4`, `libadwaita-1`.

```bash
# Build
cargo build --release

# Run
./target/release/zapret-gtk
```

---

## Türkçe
**Zapret GTK**, Linux üzerindeki [zapret](https://github.com/bol-van/zapret) DPI atlatma aracı için geliştirilmiş modern bir GTK4 arayüzüdür. Çalışan stratejileri bulma ve servisi yönetme işlemlerini son kullanıcı için basitleştirir.

### Özellikler
*   **Kolay Kurulum:** Zapret'i otomatik indirir ve kurar.
*   **Görsel Blockcheck:** DPI engellemelerine karşı çalışan yöntemleri bulmak için sihirbaz.
*   **Strateji Yönetimi:** Bulunan stratejileri listeden seçip tek tıkla uygulayın.
*   **Servis Kontrolü:** Zapret servisini başlatın, durdurun ve durumunu görün.
*   **Taşınabilir:** Tek bir dosya halinde çalışır, ek kurulum gerektirmez.

### Kurulum

#### Debian / Ubuntu / Linux Mint
Depoyu ekleyin ve paketi yükleyin:
```bash
echo "deb [trusted=yes] https://taygun86.github.io/taygun86-repo/deb ./" | sudo tee /etc/apt/sources.list.d/taygun86.list
sudo apt update
sudo apt install zapret-gtk
```

#### Fedora / OpenSUSE / RHEL
Depoyu ekleyin ve paketi yükleyin:
```bash
sudo tee /etc/yum.repos.d/taygun86.repo <<EOF
[taygun86]
name=Taygun86 Repository
baseurl=https://taygun86.github.io/taygun86-repo/rpm
enabled=1
gpgcheck=0
EOF

sudo dnf install zapret-gtk
```

#### Arch Linux / Manjaro / CachyOS / EndeavourOS
AUR üzerinden kurun:
```bash
yay -S zapret-gtk
```

### Derleme ve Çalıştırma (Manuel)
Gereksinimler: `libgtk-4`, `libadwaita-1`.

```bash
# Derle
cargo build --release

# Çalıştır
./target/release/zapret-gtk
```

---

## Русский
**Zapret GTK** — это современный графический интерфейс на базе GTK4 для инструмента обхода DPI [zapret](https://github.com/bol-van/zapret) в Linux. Приложение упрощает процесс поиска рабочих стратегий и управления фоновой службой.

### Особенности
*   **Логкая установка:** Автоматически загружает и устанавливает zapret.
*   **Графический Blockcheck:** Мастер для запуска `blockcheck` и поиска рабочих стратегий обхода.
*   **Управление стратегиями:** Легкий выбор и применение нескольких стратегий.
*   **Управление службой:** Запуск, остановка и мониторинг системной службы `zapret`.
*   **Портативность:** Один бинарный файл, не требующий внешних ресурсов.

### Установка

#### Debian / Ubuntu / Linux Mint
Добавить репозиторий и установить пакет:
```bash
echo "deb [trusted=yes] https://taygun86.github.io/taygun86-repo/deb ./" | sudo tee /etc/apt/sources.list.d/taygun86.list
sudo apt update
sudo apt install zapret-gtk
```

#### Fedora / OpenSUSE / RHEL
Добавить репозиторий и установить пакет:
```bash
sudo tee /etc/yum.repos.d/taygun86.repo <<EOF
[taygun86]
name=Taygun86 Repository
baseurl=https://taygun86.github.io/taygun86-repo/rpm
enabled=1
gpgcheck=0
EOF

sudo dnf install zapret-gtk
```

#### Arch Linux / Manjaro / CachyOS / EndeavourOS
Установка из AUR:
```bash
yay -S zapret-gtk
```

### Сборка и Запуск (Вручную)
Требования: `libgtk-4`, `libadwaita-1`.

```bash
# Сборка
cargo build --release

# Запуск
./target/release/zapret-gtk
```
