# Blockfield Launcher

<img src="assets/icon.png" alt="Blockfield" width="64" height="64" />

Лаунчер для установки, обновления и запуска Blockfield на Windows, Linux и macOS.

[Скачать](https://github.com/Blockfield/Blockfield-Launcher/releases/latest) ·
[Сайт](https://blockfield.pro) · [Сообщить об ошибке](https://github.com/Blockfield/Blockfield-Launcher/issues)

## Начать играть

1. Скачайте установщик для своей системы из [последнего релиза](https://github.com/Blockfield/Blockfield-Launcher/releases/latest).
2. Установите и откройте лаунчер. Войдите в аккаунт Blockfield или зарегистрируйтесь.
3. Дождитесь установки сборки и нажмите **«Играть»**.

Лаунчер сам загрузит Java 21, Minecraft, Fabric и модпак. Адрес сервера — `play.blockfield.pro`.

| Система               | Архитектура | Файлы в релизе              |
| :-------------------- | :---------- | :-------------------------- |
| Windows               | x86_64      | `.exe` или `.msi`           |
| Linux                 | x86_64      | `.AppImage`, `.deb`, `.rpm` |
| macOS · Intel         | x86_64      | `_x64.dmg`                  |
| macOS · Apple Silicon | M1 и новее  | `_aarch64.dmg`              |

> **На Mac:** выберите файл под свой процессор. Приложение пока не нотарифицировано Apple; после переноса в «Программы» первый запуск может потребовать разрешения в «Системные настройки → Конфиденциальность и безопасность».

## Что умеет лаунчер

|                            |                                                                         |
| :------------------------- | :---------------------------------------------------------------------- |
| **Установка и обновление** | Скачивание нужных файлов, проверка целостности и восстановление сборки. |
| **Вход в игру**            | Подготовка Java и Fabric, запуск клиента и подключение к серверу.       |
| **Настройки**              | Память для игры, язык, поведение окна и локальные команды запуска.      |
| **Подписанные обновления** | Проверка цифровой подписи перед установкой новой версии лаунчера.       |

Предпочитаете свой лаунчер? [Клиентские дистрибутивы](https://github.com/Blockfield/blockfield-releases) доступны отдельно.

## Для разработчиков

**Tauri 2 · Rust · React · TypeScript**

```sh
git clone https://github.com/Blockfield/Blockfield-Launcher.git
cd Blockfield-Launcher
just setup
just tauri-dev
```

Сначала установите Node.js из `.nvmrc`, Python 3.12.15+, Just 1.58.0, Rust 1.99.0 и системные зависимости Tauri. Полные инструкции, устройство проекта и правила выпуска — в [руководстве разработчика](docs/DEVELOPMENT.md).

Игровой стенд, прямой запуск Minecraft и testbot находятся в [workspace](https://github.com/Blockfield/blockfield-workspace/blob/main/docs/DEVELOPING.md). Связи лаунчера, сайта, Drasl и игровых репозиториев — в [общей карте архитектуры](https://github.com/Blockfield/blockfield-workspace/blob/main/docs/ARCHITECTURE.md).

[Проверка перед выпуском](docs/LAUNCHER_SMOKE.md) · [Комнаты и Discord](docs/ROOMS-AND-DISCORD.md) · [История изменений](https://github.com/Blockfield/Blockfield-Launcher/releases)

## Обратная связь и лицензия

Нашли ошибку? [Создайте issue](https://github.com/Blockfield/Blockfield-Launcher/issues) и укажите версию лаунчера, ОС и шаги воспроизведения. Перед публикацией логов удалите личные данные и токены.

Собственный код лаунчера открыт под **[GPL-3.0-only](LICENSE)**. Сторонний код и ресурсы сохраняют свои лицензии; лицензия лаунчера не распространяется на сторонний игровой контент и товарные знаки.
