---
schema_version: 1
repository: Blockfield/Blockfield-Launcher
version: launcher-v1.0.11
previous_version: launcher-v1.0.10
date: 2026-10-05
kind: launcher
backfilled: false
---

# Blockfield/Blockfield-Launcher launcher-v1.0.11

## Кратко

Лаунчер понимает адреса сервера вида `raknet;хост:порт`. Это нужно, чтобы
клиент Blockfield подключался по RakNet (UDP), а лаунчер при этом правильно
показывал статус сервера.

## Для игроков

Если адрес сервера в модпаке указан как `raknet;play.blockfield.pro:25566`,
лаунчер 1.0.11 показывает статус, онлайн и регион сервера, а не «офлайн».
Кнопка «Играть» по-прежнему сразу подключает к серверу, теперь и по RakNet.
Обновление приходит через встроенное автообновление лаунчера.

## Технические изменения

- Статус и регион: префикс `raknet;` отбрасывается, ping (SLP) идёт по TCP на
  порт 25565 того же хоста, потому что RakNet-слушатель proxy работает на UDP 25566.
- Запуск: адрес из модпака целиком, без изменений, передаётся одним аргументом
  `--quickPlayMultiplayer` и в переменную `BLOCKFIELD_SERVER`; транспорт выбирает
  клиентский мод, повтор по TCP при недоступном UDP — тоже его задача.
- Добавлены тесты разбора `raknet;` адресов (с портом, без порта, IPv6) и
  передачи адреса quick-play; обновлён `docs/DEVELOPMENT.md`.
- Версия синхронизирована в frontend, Cargo manifest, Cargo.lock и Tauri config.

## Обновлённые компоненты

Зависимости и toolchain не менялись относительно 1.0.10.

## Совместимость и необходимые действия

Поддерживаются Windows x86_64, Linux x86_64, macOS Apple Silicon и Intel.
Обычные TCP-адреса работают как раньше. Действий от игроков не требуется;
лаунчеры до 1.0.11 показывают RakNet-адрес как офлайн, поэтому модпак
переключит адрес лаунчера на RakNet только после этого выпуска.

## Известные проблемы

Без изменений относительно 1.0.10: GTK3 и ad-hoc подпись macOS без Apple
notarization, подробности — в
[примечаниях к выпуску 1.0.9](https://github.com/Blockfield/Blockfield-Launcher/releases/tag/launcher-v1.0.9).

## Источники и ограничения полноты

Сверены [PR #37](https://github.com/Blockfield/Blockfield-Launcher/pull/37)
(коммит `1ce6928`) и manifests относительно `launcher-v1.0.10`; CI PR #37
на Linux и Windows прошёл. Workflow Launcher Release собирает все платформы
и валидирует подписанные updater assets до публикации. Ручная проверка GUI,
установщика и игрового входа по RakNet через лаунчер в эту проверку не входит.
