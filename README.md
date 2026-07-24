# Handy

📖 **Languages / Языки:** English · [Русская версия ↓](#-русская-версия)

**A free, open source, and extensible speech-to-text _and_ speech-translation application that works completely offline.**
_Бесплатное, открытое и расширяемое приложение для распознавания **и перевода** речи, работающее полностью офлайн._

Handy is a cross-platform desktop application that provides simple, privacy-focused speech transcription. Press a shortcut, speak, and have your words appear in any text field. This happens on your own computer without sending any information to the cloud.

This fork goes one step further: Handy is no longer just speech-to-text — it can also **translate your speech into another language on the fly**, still fully offline, using a local LLM. See [Speech Translation](#-speech-translation-offline) below.

## Why Handy?

Handy was created to fill the gap for a truly open source, extensible speech-to-text tool. As stated on [handy.computer](https://handy.computer):

- **Free**: Accessibility tooling belongs in everyone's hands, not behind a paywall
- **Open Source**: Together we can build further. Extend Handy for yourself and contribute to something bigger
- **Private**: Your voice stays on your computer. Get transcriptions without sending audio to the cloud
- **Simple**: One tool, one job. Transcribe what you say and put it into a text box

Handy isn't trying to be the best speech-to-text app—it's trying to be the most forkable one.

## How It Works

1. **Press** a configurable keyboard shortcut to start/stop recording (or use push-to-talk mode)
2. **Speak** your words while the shortcut is active
3. **Release** and Handy processes your speech using Whisper
4. **Get** your transcribed text pasted directly into whatever app you're using

The process is entirely local:

- Silence is filtered using VAD (Voice Activity Detection) with Silero
- Transcription uses your choice of models:
  - **Whisper models** (Small/Medium/Turbo/Large) with GPU acceleration when available
  - **Parakeet V3** - CPU-optimized model with excellent performance and automatic language detection
- Works on Windows, macOS, and Linux

## 🈯 Speech Translation (Offline)

> This fork adds a dedicated **Transcribe with Translation** mode on top of Handy's dictation pipeline.

Speak in one language and have Handy paste the text **translated into a language of your choice** — fully offline. Unlike Whisper's built-in translate task (which only ever targets English), this mode can translate into **any** language by reusing Handy's LLM post-processing pipeline with a local model.

**How it works:**

1. **Press** the translation shortcut and speak
2. Handy **transcribes** your speech with your selected Whisper/Parakeet model
3. The transcript is sent to a **local LLM** (via Ollama / LM Studio) with a translation instruction
4. Only the **translation** is pasted into the active app

**Default shortcut:** `Ctrl+Alt+Space` (Windows/Linux) · `Option+Ctrl+Space` (macOS)

**Configuration** lives in **Settings → Post-Processing**:

- **Provider:** `Custom` (points to your local LLM, e.g. Ollama at `http://localhost:11434/v1`)
- **Model:** any chat model you pulled in Ollama (e.g. `qwen2.5:3b`)
- **Translation → Target language:** the language you want to translate into

The translation uses the **same local LLM provider configured for post-processing**, so nothing leaves your computer. Follow the checklist below for a step-by-step setup.

## ✅ Getting Started Checklist — Handy + Ollama

A step-by-step checklist to go from nothing to working offline dictation **and** translation. Two **separate** models are involved:

- 🗣️ a **speech-recognition (ASR) model** that lives **inside Handy** (turns your voice into text), and
- 🌍 a **translation LLM** that lives **inside Ollama** (turns that text into another language).

### Part 1 — Handy + speech recognition (dictation)

- [ ] **Install Handy** — download the [latest release](https://github.com/cjpais/Handy/releases) (Windows: `winget install cjpais.Handy`; macOS: `brew install --cask handy`)
- [ ] **Launch Handy** and grant **microphone** and **accessibility** permissions
- [ ] **Download an ASR model inside Handy** — open **Settings → Models** and download one:
  - **Example:** `Whisper Small` (~487 MB, needs a GPU) or **`Parakeet V3`** (~478 MB, runs on CPU, auto-detects language) — Parakeet is the easiest starting point
- [ ] **Test dictation** — click into any text field, press the dictation shortcut (`Ctrl+Space`), speak, release — your words should appear

### Part 2 — Ollama + translation model

- [ ] **Install Ollama** — from [ollama.com](https://ollama.com) (Windows: `winget install Ollama.Ollama`). It runs a local server at `http://localhost:11434`
- [ ] **Download a translation LLM in Ollama** — in a terminal:
  ```bash
  ollama pull qwen2.5:3b     # ~1.9 GB, strong multilingual translation
  ```
  (alternatives: `llama3.1:8b`, `gemma2:2b`, `hermes3:8b`)
- [ ] **Check Ollama is up:**
  ```bash
  curl http://localhost:11434/v1/models
  ```

### Part 3 — Connect them and translate

- [ ] In Handy: **Settings → Post-Processing → Provider** → choose **`Custom`**
- [ ] **Base URL** → `http://localhost:11434/v1` · **API Key** → leave **empty**
- [ ] **Model** → click ↻ refresh and pick `qwen2.5:3b` (or type it)
- [ ] **Translation → Target language** → choose your target (e.g. German)
- [ ] **Test translation** — click into a text field, press `Ctrl+Alt+Space`, speak in your language, release — the **translation** should be pasted

> 💡 The ASR model (Handy) and the translation LLM (Ollama) are independent. Dictation works without Ollama; translation additionally needs Ollama running with a pulled model.

## Quick Start

### Installation

1. Download the latest release from the [releases page](https://github.com/cjpais/Handy/releases) or the [website](https://handy.computer)
   - **macOS**: Also available via [Homebrew cask](https://formulae.brew.sh/cask/handy): `brew install --cask handy`
   - **Windows**: Also available via [winget](https://github.com/microsoft/winget-pkgs): `winget install cjpais.Handy` \
     **Note:** The Homebrew cask and winget package are not maintained by the Handy developers.
2. Install the application
3. Launch Handy and grant necessary system permissions (microphone, accessibility)
4. Configure your preferred keyboard shortcuts in Settings
5. Start transcribing!

### Development Setup

For detailed build instructions including platform-specific requirements, see [BUILD.md](BUILD.md).

## Integrations

<a href="https://www.raycast.com/mattiacolombomc/handy" title="Install Handy Raycast Extension"><img src="https://www.raycast.com/mattiacolombomc/handy/install_button@2x.png?v=1.1" height="64" style="height: 64px;" alt="Install handy Raycast Extension" /></a>

Control Handy from [Raycast](https://www.raycast.com) — start/stop recording, browse transcript history, manage dictionary, switch models and languages.

[Source](https://github.com/mattiacolombomc/raycast-handy) · by [@mattiacolombomc](https://github.com/mattiacolombomc)

## Architecture

Handy is built as a Tauri application combining:

- **Frontend**: React + TypeScript with Tailwind CSS for the settings UI
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core Libraries**:
  - `transcribe-cpp`: Local speech recognition with Whisper-family models (GGML/GGUF)
  - `transcribe-rs`: CPU-optimized speech recognition with Parakeet models
  - `cpal`: Cross-platform audio I/O
  - `vad-rs`: Voice Activity Detection
  - `rdev`: Global keyboard shortcuts and system events
  - `rubato`: Audio resampling

### Debug Mode

Handy includes an advanced debug mode for development and troubleshooting. Access it by pressing:

- **macOS**: `Cmd+Shift+D`
- **Windows/Linux**: `Ctrl+Shift+D`

### CLI Parameters

Handy supports command-line flags for controlling a running instance and customizing startup behavior. These work on all platforms (macOS, Windows, Linux).

**Remote control flags** (sent to an already-running instance via the single-instance plugin):

```bash
handy --toggle-transcription    # Toggle recording on/off
handy --toggle-post-process     # Toggle recording with post-processing on/off
handy --cancel                  # Cancel the current operation
```

**Startup flags:**

```bash
handy --start-hidden            # Start without showing the main window
handy --no-tray                 # Start without the system tray icon
handy --debug                   # Enable debug mode with verbose logging
handy --help                    # Show all available flags
```

Flags can be combined for autostart scenarios:

```bash
handy --start-hidden --no-tray
```

> **macOS tip:** When Handy is installed as an app bundle, invoke the binary directly:
>
> ```bash
> /Applications/Handy.app/Contents/MacOS/Handy --toggle-transcription
> ```

## Known Issues & Current Limitations

This project is actively being developed and has some [known issues](https://github.com/cjpais/Handy/issues). We believe in transparency about the current state:

### Major Issues (Help Wanted)

**Whisper Model Crashes:**

- Whisper models crash on certain system configurations (Windows and Linux)
- Does not affect all systems - issue is configuration-dependent
  - If you experience crashes and are a developer, please help to fix and provide debug logs!

**Wayland Support (Linux):**

- Limited support for Wayland display server
- Requires [`wtype`](https://github.com/atx/wtype) or [`dotool`](https://sr.ht/~geb/dotool/) for text input to work correctly (see [Linux Notes](#linux-notes) below for installation)

### Linux Notes

**Text Input Tools:**

For reliable text input on Linux, install the appropriate tool for your display server:

| Display Server | Recommended Tool | Install Command                                    |
| -------------- | ---------------- | -------------------------------------------------- |
| X11            | `xdotool`        | `sudo apt install xdotool`                         |
| Wayland        | `wtype`          | `sudo apt install wtype`                           |
| Both           | `dotool`         | `sudo apt install dotool` (requires `input` group) |

- **X11**: Install `xdotool` for both direct typing and clipboard paste shortcuts
- **Wayland**: Install `wtype` (preferred) or `dotool` for text input to work correctly
- **dotool setup**: Requires adding your user to the `input` group: `sudo usermod -aG input $USER` (then log out and back in)

Without these tools, Handy falls back to enigo which may have limited compatibility, especially on Wayland.

**Other Notes:**

- **Runtime library dependency (`libgtk-layer-shell.so.0`)**:
  - Handy links `gtk-layer-shell` on Linux. If startup fails with `error while loading shared libraries: libgtk-layer-shell.so.0`, install the runtime package for your distro:

    | Distro        | Package to install    | Example command                        |
    | ------------- | --------------------- | -------------------------------------- |
    | Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
    | Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
    | Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

  - For building from source on Ubuntu/Debian, you may also need `libgtk-layer-shell-dev`.

- The recording overlay is disabled by default on Linux (`Overlay Position: None`) because certain compositors treat it as the active window. When the overlay is visible it can steal focus, which prevents Handy from pasting back into the application that triggered transcription. If you enable the overlay anyway, be aware that clipboard-based pasting might fail or end up in the wrong window.
- If you are having trouble with the app, running with the environment variable `WEBKIT_DISABLE_DMABUF_RENDERER=1` may help
- If Handy fails to start reliably on Linux, see [Troubleshooting → Linux Startup Crashes or Instability](#linux-startup-crashes-or-instability).
- **Global keyboard shortcuts (Wayland):** On Wayland, system-level shortcuts must be configured through your desktop environment or window manager. Use the [CLI flags](#cli-parameters) as the command for your custom shortcut.

  **GNOME:**
  1. Open **Settings > Keyboard > Keyboard Shortcuts > Custom Shortcuts**
  2. Click the **+** button to add a new shortcut
  3. Set the **Name** to `Toggle Handy Transcription`
  4. Set the **Command** to `handy --toggle-transcription`
  5. Click **Set Shortcut** and press your desired key combination (e.g., `Super+O`)

  **KDE Plasma:**
  1. Open **System Settings > Shortcuts > Custom Shortcuts**
  2. Click **Edit > New > Global Shortcut > Command/URL**
  3. Name it `Toggle Handy Transcription`
  4. In the **Trigger** tab, set your desired key combination
  5. In the **Action** tab, set the command to `handy --toggle-transcription`

  **Sway / i3:**

  Add to your config file (`~/.config/sway/config` or `~/.config/i3/config`):

  ```ini
  bindsym $mod+o exec handy --toggle-transcription
  ```

  **Hyprland:**

  Add to your config file (`~/.config/hypr/hyprland.conf`):

  ```ini
  bind = $mainMod, O, exec, handy --toggle-transcription
  ```

- You can also manage global shortcuts outside of Handy via Unix signals, which lets Wayland window managers or other hotkey daemons keep ownership of keybindings:

  | Signal    | Action                                    | Example                |
  | --------- | ----------------------------------------- | ---------------------- |
  | `SIGUSR2` | Toggle transcription                      | `pkill -USR2 -n handy` |
  | `SIGUSR1` | Toggle transcription with post-processing | `pkill -USR1 -n handy` |

  Example Sway config:

  ```ini
  bindsym $mod+o exec pkill -USR2 -n handy
  bindsym $mod+p exec pkill -USR1 -n handy
  ```

  `pkill` here simply delivers the signal—it does not terminate the process.

**Overlay & Pasting Issues (Linux):**

- The recording overlay window can interfere with pasting transcribed text into target applications on Linux (X11)
- **Solution:** Open **Settings > Advanced** and set **"Overlay Position"** to **"None"** to disable the overlay
- Enable **"Audio Feedback"** (also in Advanced) if you still want audible confirmation of recording state
- Users who upgrade from older versions or import settings from other platforms may need to manually apply this change

### Platform Support

- **macOS (both Intel and Apple Silicon)**
- **x64 Windows**
- **x64 Linux**

### System Requirements/Recommendations

The following are recommendations for running Handy on your own machine. If you don't meet the system requirements, the performance of the application may be degraded. We are working on improving the performance across all kinds of computers and hardware.

**For Whisper Models:**

- **macOS**: M series Mac, Intel Mac
- **Windows**: Intel, AMD, or NVIDIA GPU
- **Linux**: Intel, AMD, or NVIDIA GPU
  - Ubuntu 22.04, 24.04

**For Parakeet V3 Model:**

- **CPU-only operation** - runs on a wide variety of hardware
- **Minimum**: Intel Skylake (6th gen) or equivalent AMD processors
- **Performance**: ~5x real-time speed on mid-range hardware (tested on i5)
- **Automatic language detection** - no manual language selection required

## Roadmap & Active Development

We're actively working on several features and improvements. Contributions and feedback are welcome!

### In Progress

**Debug Logging:**

- Adding debug logging to a file to help diagnose issues

**macOS Keyboard Improvements:**

- Support for Globe key as transcription trigger
- A rewrite of global shortcut handling for MacOS, and potentially other OS's too.

**Opt-in Analytics:**

- Collect anonymous usage data to help improve Handy
- Privacy-first approach with clear opt-in

**Settings Refactoring:**

- Cleanup and refactor settings system which is becoming bloated and messy
- Implement better abstractions for settings management

**Tauri Commands Cleanup:**

- Abstract and organize Tauri command patterns
- Investigate tauri-specta for improved type safety and organization

## Verify Release Signatures

Handy release artifacts are signed with Tauri's updater signature format. The public key is stored in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) under `plugins.updater.pubkey`.

To verify a release manually, set `ARTIFACT` to the filename you downloaded, save the `pubkey` value from `src-tauri/tauri.conf.json` to `handy.pub.b64`, then decode the public key and matching `.sig` file from base64 and verify the artifact with `minisign`:

```bash
# Replace with the file you downloaded
ARTIFACT="Handy_0.8.1_amd64.AppImage"

python3 - "$ARTIFACT" <<'PY'
import base64, pathlib, sys

artifact = sys.argv[1]

pub = pathlib.Path("handy.pub.b64").read_text().strip()
pathlib.Path("handy.pub").write_bytes(base64.b64decode(pub))

sig = pathlib.Path(f"{artifact}.sig").read_text().strip()
pathlib.Path(f"{artifact}.minisig").write_bytes(base64.b64decode(sig))
PY

minisign -Vm "$ARTIFACT" \
  -p handy.pub \
  -x "$ARTIFACT.minisig"
```

On success, `minisign` prints:

```text
Signature and comment signature verified
```

Do not use `gpg` for these `.sig` files.

## Troubleshooting

### Manual Model Installation (For Proxy Users or Network Restrictions)

If you're behind a proxy, firewall, or in a restricted network environment where Handy cannot download models automatically, you can manually download and install them. The URLs are publicly accessible from any browser.

#### Step 1: Find Your App Data Directory

1. Open Handy settings
2. Navigate to the **About** section
3. Copy the "App Data Directory" path shown there, or use the shortcuts:
   - **macOS**: `Cmd+Shift+D` to open debug menu
   - **Windows/Linux**: `Ctrl+Shift+D` to open debug menu

The typical paths are:

- **macOS**: `~/Library/Application Support/com.pais.handy/`
- **Windows**: `C:\Users\{username}\AppData\Roaming\com.pais.handy\`
- **Linux**: `~/.config/com.pais.handy/`

#### Step 2: Create Models Directory

Inside your app data directory, create a `models` folder if it doesn't already exist:

```bash
# macOS/Linux
mkdir -p ~/Library/Application\ Support/com.pais.handy/models

# Windows (PowerShell)
New-Item -ItemType Directory -Force -Path "$env:APPDATA\com.pais.handy\models"
```

#### Step 3: Download Model Files

Download the models you want from below

**Whisper Models (single .bin files):**

- Small (487 MB): `https://blob.handy.computer/ggml-small.bin`
- Medium (492 MB): `https://blob.handy.computer/whisper-medium-q4_1.bin`
- Turbo (1600 MB): `https://blob.handy.computer/ggml-large-v3-turbo.bin`
- Large (1100 MB): `https://blob.handy.computer/ggml-large-v3-q5_0.bin`

**Parakeet Models (compressed archives):**

- V2 (473 MB): `https://blob.handy.computer/parakeet-v2-int8.tar.gz`
- V3 (478 MB): `https://blob.handy.computer/parakeet-v3-int8.tar.gz`

#### Step 4: Install Models

**For Whisper Models (.bin files):**

Simply place the `.bin` file directly into the `models` directory:

```
{app_data_dir}/models/
├── ggml-small.bin
├── whisper-medium-q4_1.bin
├── ggml-large-v3-turbo.bin
└── ggml-large-v3-q5_0.bin
```

**For Parakeet Models (.tar.gz archives):**

1. Extract the `.tar.gz` file
2. Place the **extracted directory** into the `models` folder
3. The directory must be named exactly as follows:
   - **Parakeet V2**: `parakeet-tdt-0.6b-v2-int8`
   - **Parakeet V3**: `parakeet-tdt-0.6b-v3-int8`

Final structure should look like:

```
{app_data_dir}/models/
├── parakeet-tdt-0.6b-v2-int8/     (directory with model files inside)
│   ├── (model files)
│   └── (config files)
└── parakeet-tdt-0.6b-v3-int8/     (directory with model files inside)
    ├── (model files)
    └── (config files)
```

**Important Notes:**

- For Parakeet models, the extracted directory name **must** match exactly as shown above
- Do not rename the `.bin` files for Whisper models—use the exact filenames from the download URLs
- After placing the files, restart Handy to detect the new models

#### Step 5: Verify Installation

1. Restart Handy
2. Open Settings → Models
3. Your manually installed models should now appear as "Downloaded"
4. Select the model you want to use and test transcription

### Custom Whisper Models

Handy can auto-discover custom Whisper GGML models placed in the `models` directory. This is useful for users who want to use fine-tuned or community models not included in the default model list.

**How to use:**

1. Obtain a Whisper model in GGML `.bin` format (e.g., from [Hugging Face](https://huggingface.co/models?search=whisper%20ggml))
2. Place the `.bin` file in your `models` directory (see paths above)
3. Restart Handy to discover the new model
4. The model will appear in the "Custom Models" section of the Models settings page

**Important:**

- Community models are user-provided and may not receive troubleshooting assistance
- The model must be a valid Whisper GGML format (`.bin` file)
- Model name is derived from the filename (e.g., `my-custom-model.bin` → "My Custom Model")

### Linux Startup Crashes or Instability

If Handy fails to start reliably on Linux — for example, it crashes shortly after launch, never shows its window, or reports a Wayland protocol error — try the steps below in order.

**1. Install (or reinstall) `gtk-layer-shell`**

Handy uses `gtk-layer-shell` for its recording overlay and links against it at runtime. A missing or broken installation is the most common cause of startup failures and can manifest as a crash or a hang well before any window is shown. Make sure the runtime package is installed for your distro:

| Distro        | Package to install    | Example command                        |
| ------------- | --------------------- | -------------------------------------- |
| Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
| Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
| Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

If it is already installed and you still see startup problems, try reinstalling it (e.g. `sudo pacman -S gtk-layer-shell` again) in case the library files were corrupted by a partial upgrade.

**2. Disable the GTK layer shell overlay (`HANDY_NO_GTK_LAYER_SHELL`)**

If installing the library does not help, you can skip `gtk-layer-shell` initialization entirely as a workaround. On some compositors (notably KDE Plasma under Wayland) it has been reported to interact poorly with the recording overlay. With this variable set, the overlay falls back to a regular always-on-top window:

```bash
HANDY_NO_GTK_LAYER_SHELL=1 handy
```

**3. Disable WebKit DMA-BUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER`)**

On some GPU/driver combinations the WebKitGTK DMA-BUF renderer can cause the window to fail to render or to crash. Try:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 handy
```

**Making a workaround permanent**

Once you've found a flag that helps, export it from your shell profile (`~/.bashrc`, `~/.zshenv`, …) or from the desktop autostart entry that launches Handy. If you launch Handy from a `.desktop` file, you can prefix the `Exec=` line, e.g.:

```ini
Exec=env HANDY_NO_GTK_LAYER_SHELL=1 handy
```

If a workaround helps you, please [open an issue](https://github.com/cjpais/Handy/issues) describing your distro, desktop environment, and session type — that information helps us narrow down the underlying bug.

### How to Contribute

1. **Check existing issues** at [github.com/cjpais/Handy/issues](https://github.com/cjpais/Handy/issues)
2. **Fork the repository** and create a feature branch
3. **Test thoroughly** on your target platform
4. **Submit a pull request** with clear description of changes
5. **Join the discussion** - reach out at [contact@handy.computer](mailto:contact@handy.computer)

The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community.

## Sponsors

<div align="center">
  We're grateful for the support of our sponsors who help make Handy possible:
  <br><br>
  <a href="https://wordcab.com">
    <img src="sponsor-images/wordcab.png" alt="Wordcab" width="120" height="120">
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://github.com/epicenter-so/epicenter">
    <img src="sponsor-images/epicenter.png" alt="Epicenter" width="120" height="120">
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://boltai.com?utm_source=handy">
    <img src="sponsor-images/boltai.jpg" alt="Bolt AI" width="120" height="120">
  </a>
</div>

## Related Projects

- **[Handy CLI](https://github.com/cjpais/handy-cli)** - The original Python command-line version
- **[handy.computer](https://handy.computer)** - Project website with demos and documentation

## License

MIT License - see [LICENSE](LICENSE) file for details.

Handy is open-source software, but the Handy name, logo, icon, and brand assets are not open-source. Unofficial forks, rewrites, and redistributions must use their own branding and must not imply endorsement or affiliation.

## Acknowledgments

- **Whisper** by OpenAI for the speech recognition model
- **ggml and transcribe.cpp** for amazing cross-platform speech-to-text inference/acceleration
- **Silero** for great lightweight VAD
- **Tauri** team for the excellent Rust-based app framework
- **Community contributors** helping make Handy better

---

## 🇷🇺 Русская версия

> Полная англоязычная документация — выше. Здесь переведены основные разделы и добавлены разделы про оффлайн-перевод и пошаговый запуск. Команды, пути и названия настроек в приложении одинаковы для всех языков.

### Handy

**Бесплатное, открытое и расширяемое приложение для распознавания _и перевода_ речи, работающее полностью офлайн.**

Handy — кроссплатформенное десктоп-приложение для простого и приватного распознавания речи. Нажмите шорткат, скажите фразу — и текст появится в любом текстовом поле. Всё происходит на вашем компьютере, без отправки данных в облако.

Этот форк идёт дальше: Handy теперь не только распознаёт речь, но и умеет **переводить сказанное на другой язык на лету** — по-прежнему полностью офлайн, с помощью локальной LLM. См. раздел [Перевод речи (оффлайн)](#-перевод-речи-оффлайн) ниже.

### Почему Handy?

- **Бесплатно** — доступные инструменты должны быть у всех, а не за пейволлом
- **Открытый код** — вместе можно построить больше: дорабатывайте Handy под себя и вносите вклад
- **Приватно** — ваш голос остаётся на вашем компьютере, аудио не уходит в облако
- **Просто** — один инструмент, одна задача: распознать сказанное и вставить в текстовое поле

### Как это работает

1. **Нажмите** настраиваемый шорткат, чтобы начать/остановить запись (или используйте режим push-to-talk)
2. **Говорите**, пока шорткат активен
3. **Отпустите** — Handy распознаёт речь
4. **Готово** — распознанный текст вставляется в активное приложение

Полностью локально: тишина отсекается через VAD (Silero), распознавание — моделями **Whisper** (Small/Medium/Turbo/Large, с GPU-ускорением) или **Parakeet V3** (оптимизирован под CPU, автоопределение языка). Работает на Windows, macOS и Linux.

### 🈯 Перевод речи (оффлайн)

> Этот форк добавляет отдельный режим **«Transcribe with Translation»** поверх обычной диктовки.

Говорите на одном языке — Handy вставит текст, **переведённый на выбранный вами язык**, полностью офлайн. В отличие от встроенного перевода Whisper (только на английский), этот режим переводит на **любой** язык, переиспользуя пайплайн post-processing с локальной LLM.

**Как работает:**

1. Нажмите шорткат перевода и говорите
2. Handy распознаёт речь выбранной моделью (Whisper/Parakeet)
3. Текст уходит в **локальную LLM** (через Ollama / LM Studio) с инструкцией на перевод
4. В активное приложение вставляется **только перевод**

**Шорткат по умолчанию:** `Ctrl+Alt+Space` (Windows/Linux) · `Option+Ctrl+Space` (macOS)

**Настройки** — **Settings → Post-Processing**:

- **Provider:** `Custom` (ваша локальная LLM, напр. Ollama на `http://localhost:11434/v1`)
- **Model:** любая chat-модель, скачанная в Ollama (напр. `qwen2.5:3b`)
- **Translation → Target language:** язык, на который переводить

Перевод использует **тот же локальный провайдер, что и post-processing** — ничего не покидает компьютер.

### ✅ Чек-лист: развернуть Handy + Ollama с нуля

Задействованы **две разные модели**:

- 🗣️ **модель распознавания речи (ASR)** — живёт **внутри Handy** (голос → текст)
- 🌍 **LLM для перевода** — живёт **внутри Ollama** (текст → другой язык)

#### Часть 1 — Handy и распознавание речи (диктовка)

- [ ] **Установить Handy** — [последний релиз](https://github.com/cjpais/Handy/releases) (Windows: `winget install cjpais.Handy`; macOS: `brew install --cask handy`)
- [ ] **Запустить Handy**, выдать доступ к **микрофону** и **специальным возможностям** (accessibility)
- [ ] **Скачать ASR-модель в самом Handy** — **Settings → Models**, скачать одну:
  - **Пример:** `Whisper Small` (~487 МБ, нужен GPU) или **`Parakeet V3`** (~478 МБ, работает на CPU, автоопределение языка) — Parakeet проще всего для старта
- [ ] **Проверить диктовку** — поставить курсор в любое текстовое поле, нажать шорткат (`Ctrl+Space`), сказать фразу, отпустить — должен появиться текст

#### Часть 2 — Ollama и модель перевода

- [ ] **Установить Ollama** — с [ollama.com](https://ollama.com) (Windows: `winget install Ollama.Ollama`). Поднимает локальный сервер на `http://localhost:11434`
- [ ] **Скачать LLM для перевода** — в терминале:
  ```bash
  ollama pull qwen2.5:3b     # ~1.9 ГБ, сильная многоязычная модель
  ```
  (альтернативы: `llama3.1:8b`, `gemma2:2b`, `hermes3:8b`)
- [ ] **Проверить, что Ollama жива:**
  ```bash
  curl http://localhost:11434/v1/models
  ```

#### Часть 3 — Связать и перевести

- [ ] В Handy: **Settings → Post-Processing → Provider** → выбрать **`Custom`**
- [ ] **Base URL** → `http://localhost:11434/v1` · **API Key** → оставить **пустым**
- [ ] **Model** → нажать ↻ (refresh) и выбрать `qwen2.5:3b` (или вписать вручную)
- [ ] **Translation → Target language** → выбрать целевой язык (напр. немецкий)
- [ ] **Проверить перевод** — курсор в текстовое поле, нажать `Ctrl+Alt+Space`, сказать фразу на своём языке, отпустить — вставится **перевод**

> 💡 ASR-модель (в Handy) и LLM перевода (в Ollama) независимы. Диктовка работает без Ollama; для перевода дополнительно нужна запущенная Ollama со скачанной моделью.

### Требования к системе (кратко)

- **Whisper-модели:** macOS (M-серия / Intel), Windows и Linux с GPU (Intel / AMD / NVIDIA)
- **Parakeet V3:** только CPU, от Intel Skylake (6-е поколение) или аналога AMD; ~5× реального времени на среднем железе; автоопределение языка

### Остальное

Разделы про **CLI-флаги**, **известные проблемы**, **заметки для Linux**, **проверку подписей релизов** и **устранение неполадок** смотрите в англоязычной части выше — команды и пути в них одинаковы независимо от языка интерфейса.

### Сборка из исходников

Инструкция по сборке под конкретную платформу — в [BUILD.md](BUILD.md) (там есть русская версия).
