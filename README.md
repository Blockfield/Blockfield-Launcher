<div align="center">

<img src="src-tauri/icons/128x128@2x.png" alt="Blockfield" width="88" height="88" />

# BLOCKFIELD LAUNCHER

**От установки до первого матча.**<br />
Официальный лаунчер Blockfield для Windows, Linux и macOS.

[![Релиз](https://img.shields.io/github/v/release/Blockfield/Blockfield-Launcher?label=release&color=E9A426&style=flat-square)](https://github.com/Blockfield/Blockfield-Launcher/releases/latest)
[![Сборка](https://github.com/Blockfield/Blockfield-Launcher/actions/workflows/release.yml/badge.svg)](https://github.com/Blockfield/Blockfield-Launcher/actions/workflows/release.yml)
[![GPL-3.0-only](https://img.shields.io/badge/license-GPL--3.0--only-E9A426?style=flat-square)](LICENSE)

**[Скачать лаунчер](https://github.com/Blockfield/Blockfield-Launcher/releases/latest)** · [Сайт проекта](https://blockfield.pro) · [Сообщить об ошибке](https://github.com/Blockfield/Blockfield-Launcher/issues)

</div>

---

## Начать играть

1. Скачайте установщик для своей системы из [последнего релиза](https://github.com/Blockfield/Blockfield-Launcher/releases/latest).
2. Установите и откройте лаунчер, выберите игровой ник.
3. Дождитесь установки сборки и нажмите **«Играть»**.

Лаунчер сам загрузит Java 21, Minecraft, Fabric и модпак. Адрес сервера — `play.blockfield.pro`.

| Система | Архитектура | Файлы в релизе |
| :--- | :--- | :--- |
| Windows | x86_64 | `.exe` или `.msi` |
| Linux | x86_64 | `.AppImage`, `.deb`, `.rpm` |
| macOS · Intel | x86_64 | `_x64.dmg` |
| macOS · Apple Silicon | M1 и новее | `_aarch64.dmg` |

> **На Mac:** выберите файл под свой процессор. Приложение пока не нотарифицировано Apple; после переноса в «Программы» первый запуск может потребовать разрешения в «Системные настройки → Конфиденциальность и безопасность».

## Что умеет лаунчер

| | |
| :--- | :--- |
| **Установка и обновление** | Скачивание нужных файлов, проверка целостности и восстановление сборки. |
| **Вход в игру** | Подготовка Java и Fabric, запуск клиента и подключение к серверу. |
| **Настройки** | Память для игры, язык, поведение окна и локальные команды запуска. |
| **Подписанные обновления** | Проверка цифровой подписи перед установкой новой версии лаунчера. |

Предпочитаете свой лаунчер? [Клиентские дистрибутивы](https://github.com/netherg-io/blockfield-releases) доступны отдельно.

## Для разработчиков

**Tauri 2 · Rust · React · TypeScript**

```sh
git clone https://github.com/Blockfield/Blockfield-Launcher.git
cd Blockfield-Launcher
cp .env.example .env
pnpm install --frozen-lockfile
pnpm tauri:dev
```

Сначала установите Node.js из `.nvmrc`, pnpm 11, Rust и системные зависимости Tauri. Полные инструкции, устройство проекта и правила выпуска — в [руководстве разработчика](docs/DEVELOPMENT.md).

[Проверка перед выпуском](docs/LAUNCHER_SMOKE.md) · [Комнаты и Discord](docs/ROOMS-AND-DISCORD.md) · [История изменений](https://github.com/Blockfield/Blockfield-Launcher/releases)

## Обратная связь и лицензия

Нашли ошибку? [Создайте issue](https://github.com/Blockfield/Blockfield-Launcher/issues) и укажите версию лаунчера, ОС и шаги воспроизведения. Перед публикацией логов удалите личные данные и токены.

Собственный код лаунчера открыт под **[GPL-3.0-only](LICENSE)**. Сторонний код и ресурсы сохраняют свои лицензии; лицензия лаунчера не распространяется на сторонний игровой контент и товарные знаки.

---

<div align="center">

[**BLOCKFIELD**](https://blockfield.pro) · Командные бои в Minecraft 1.21.1 · Fabric

</div>
