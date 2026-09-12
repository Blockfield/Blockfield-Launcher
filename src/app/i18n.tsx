import { createContext, useContext } from 'react'

export type Lang = 'ru'

const ru = {
  'chrome.title': 'ЛАУНЧЕР BLOCKFIELD',

  'login.launcher': 'ЛАУНЧЕР',
  'login.authRequired': 'ТРЕБУЕТСЯ АВТОРИЗАЦИЯ ОПЕРАТОРА',
  'login.callsign': 'ПОЗЫВНОЙ / EMAIL',
  'login.accessKey': 'КЛЮЧ ДОСТУПА',
  'login.remember': 'ЗАПОМНИТЬ ОПЕРАТОРА',
  'login.forgot': 'ЗАБЫЛИ КЛЮЧ?',
  'login.createAccount': 'СОЗДАТЬ АККАУНТ',
  'login.haveAccount': 'НАЗАД КО ВХОДУ',
  'login.register': 'ЗАРЕГИСТРИРОВАТЬСЯ',
  'login.email': 'EMAIL',
  'login.confirmKey': 'ПОВТОРИТЕ КЛЮЧ',
  'login.resetHelp': 'СБРОС ПАРОЛЯ ДОСТУПЕН ЧЕРЕЗ АДМИНИСТРАТОРА СЕРВЕРА.',
  'login.registrationFailed': 'ПРОВЕРЬТЕ ДАННЫЕ АККАУНТА И ПОВТОРИТЕ',
  'login.signIn': 'ВОЙТИ',
  'login.ready': 'ЛАУНЧЕР ГОТОВ',
  'login.invalidCredentials': 'НЕВЕРНЫЙ ПОЗЫВНОЙ ИЛИ КЛЮЧ',
  'login.serviceUnavailable': 'СЕРВИС АВТОРИЗАЦИИ НЕДОСТУПЕН',
  'login.build': 'СБОРКА {v} — СТАБИЛЬНАЯ',
  'login.sector': 'СЕКТОР 07 — СЕВЕРНЫЙ ХРЕБЕТ',
  'login.slogan': '«ВЫСАДКА. ЗАХВАТ. ДОМИНАЦИЯ.»',

  'nav.deploy': 'ИГРАТЬ',
  'nav.updates': 'ОБНОВЛЕНИЯ',
  'nav.settings': 'НАСТРОЙКИ',
  'shell.logout': 'Выйти',
  'shell.support': 'ПОДДЕРЖКА',
  'shell.launcherVersion': 'ЛАУНЧЕР v{v}',
  'shell.ip': 'IP · {ip}',

  'main.operation': 'ИГРА',
  'main.active': 'АКТИВНА',
  'main.season': 'Minecraft 1.21.1 · Fabric',
  'main.description':
    'Командные бои с захватом точек. Выбирайте доступный класс и снаряжение, занимайте позиции и сохраняйте билеты своей команды.',
  'main.enterBattlefield': 'ПОДКЛЮЧЕНИЕ К СЕРВЕРУ',
  'main.modpack': 'МОДПАК',
  'main.upToDate': 'АКТУАЛЕН',
  'main.auth': 'ВХОД',
  'main.verified': 'ПО НИКНЕЙМУ',
  'main.players': 'ИГРОКИ',
  'main.ping': 'ПИНГ',
  'main.region': 'РЕГИОН',
  'main.server': 'СЕРВЕР',
  'main.online': 'ОНЛАЙН',
  'main.serverName': 'BLOCKFIELD · ОСНОВНОЙ',
  'main.briefing': 'ОБ ИГРЕ',
  'main.feat.capture': 'ЗАХВАТ ТОЧЕК',
  'main.feat.captureDesc': 'Сражайтесь за контроль точек и сохраняйте билеты своей команды.',
  'main.feat.classes': 'КЛАССЫ',
  'main.feat.classesDesc':
    'Штурмовик и марксман. Доступность инженера, снайпера и оператора FPV зависит от настроек и числа игроков.',
  'main.feat.loadout': 'СНАРЯЖЕНИЕ',
  'main.feat.loadoutDesc': 'Выбирайте комплект оружия из вариантов, доступных вашему классу.',
  'main.feat.menu': 'ИГРОВОЕ МЕНЮ',
  'main.feat.menuDesc': 'Выбирайте комнату, класс и точку появления через меню в игре.',
  'main.modpackStatus': 'СТАТУС МОДПАКА',
  'main.installed': 'УСТАНОВЛЕН',
  'main.latest': 'ПОСЛЕДНИЙ',
  'main.size': 'РАЗМЕР',
  'main.autoUpdate': 'СОСТОЯНИЕ',
  'main.enabled': 'ВКЛ',
  'main.disabled': 'ВЫКЛ',
  'main.fieldReport': 'ПОЛЕВОЙ ОТЧЁТ',
  'main.viewLog': 'ОТКРЫТЬ ЖУРНАЛ ОПЕРАЦИЙ',
  'main.tag.patch': 'ПАТЧ',
  'main.tag.event': 'ИВЕНТ',
  'main.tag.ops': 'ОПС',

  'update.packageSync': 'ПОДГОТОВКА ИГРЫ',
  'update.updating': 'ОБНОВЛЕНИЕ МОДПАКА',
  'update.patch': '/ ПАТЧ {v}',
  'update.description':
    'Лаунчер скачает обновления и проверит файлы игры. Дождитесь завершения перед запуском.',
  'update.mirror': 'ЗЕРКАЛО',
  'update.locked': 'ЗАБЛОКИРОВАНО',
  'update.inProgress': 'ИДЁТ ОБНОВЛЕНИЕ',
  'update.completion': 'ПРОГРЕСС',
  'update.currentFile': 'ТЕКУЩИЙ ФАЙЛ',
  'update.transferred': 'ЗАГРУЖЕНО',
  'update.remaining': 'ОСТАЛОСЬ',
  'update.throughput': 'СКОРОСТЬ',
  'update.status': 'СОСТОЯНИЕ ОБНОВЛЕНИЯ',
  'update.integrity': 'ПРОВЕРКА ФАЙЛОВ',
  'update.opLog': 'ЖУРНАЛ ОБНОВЛЕНИЯ',
  'update.steps': 'ЭТАПЫ ЗАПУСКА',
  'update.step.verify': 'ПРОВЕРКА СПИСКА ФАЙЛОВ',
  'update.step.setup': 'УСТАНОВКА КОМПОНЕНТОВ',
  'update.step.prune': 'ОЧИСТКА СТАРЫХ ФАЙЛОВ',
  'update.step.download': 'ЗАГРУЗКА ПАКЕТА',
  'update.step.runtime': 'СРЕДА MINECRAFT',
  'update.step.integrity': 'ПРОВЕРКА ЦЕЛОСТНОСТИ',
  'update.step.finalize': 'ЗАВЕРШЕНИЕ УСТАНОВКИ',
  'update.step.java': 'СРЕДА JAVA',
  'update.step.done': 'ГОТОВО',
  'update.step.running': 'В РАБОТЕ',
  'update.step.queued': 'В ОЧЕРЕДИ',
  'update.eta': '~ 1м 42с',

  'settings.configuration': 'КОНФИГУРАЦИЯ',
  'settings.operator': 'ОПЕРАТОР · {handle}',
  'settings.preferences': '/ ПАРАМЕТРЫ ЛАУНЧЕРА',
  'settings.tab.general': 'ОБЩИЕ',
  'settings.tab.runtime': 'JAVA И ПАМЯТЬ',
  'settings.tab.launcher': 'ЛАУНЧЕР',
  'settings.tab.commands': 'КОМАНДЫ',
  'settings.runtime': 'СРЕДА',
  'settings.launcher': 'ЛАУНЧЕР',
  'settings.gameDir': 'ПАПКА ИГРЫ',
  'settings.gameDirHint': 'Расположение всех игровых файлов и профилей игрока.',
  'settings.java': 'СРЕДА JAVA',
  'settings.javaHint': 'Путь необязателен — лаунчер выберет Java автоматически.',
  'settings.javaAuto': 'Автоматически',
  'settings.ram': 'ВЫДЕЛЕНИЕ RAM',
  'settings.ramHint': '{gb} ГБ выделено · по умолчанию 4 ГБ',
  'settings.language': 'ЯЗЫК',
  'settings.languageHint': 'Язык интерфейса лаунчера.',
  'settings.autoUpdate': 'ПРОВЕРКА ПРИ СТАРТЕ',
  'settings.autoUpdateHint':
    'Проверять и восстанавливать файлы установленной сборки при открытии лаунчера.',
  'settings.commands': 'КОМАНДЫ ЗАПУСКА',
  'settings.preLaunch': 'ПЕРЕД ЗАПУСКОМ',
  'settings.preLaunchHint': 'Выполняется перед Minecraft. При ошибке запуск игры отменяется.',
  'settings.postExit': 'ПОСЛЕ ВЫХОДА ИЗ ИГРЫ',
  'settings.postExitHint':
    'Выполняется после завершения процесса Minecraft, в том числе при сбое. Лаунчер должен оставаться открытым.',
  'settings.commandPlaceholder': 'Не выполнять команду',
  'settings.commandsHint':
    'Команды выполняются в папке игры через sh (Linux/macOS) или cmd (Windows). Доступны INST_DIR, INST_MC_DIR, INST_JAVA и INST_EXIT_CODE после выхода. Обращение к переменным: $INST_DIR в sh, %INST_DIR% в cmd. Вывод: logs/launcher-hooks.log.',
  'main.verifyFiles': 'ПРОВЕРИТЬ ФАЙЛЫ',
  'main.prepare': 'ПОДГОТОВИТЬ',
  'main.setupRequired': 'ТРЕБУЕТСЯ ПОДГОТОВКА',
  'main.updateAction': 'ОБНОВИТЬ',
  'settings.logout': 'ВЫЙТИ ИЗ ПРОФИЛЯ',
  'settings.username': 'НИКНЕЙМ',
  'settings.usernameHint': 'Ник в Minecraft (3-16 букв, цифр или _).',
  'shell.offlineMode': 'ЛОКАЛЬНЫЙ ПРОФИЛЬ',
  'settings.reset': 'СБРОС',
  'settings.save': 'СОХРАНИТЬ',
  'settings.browse': 'ОБЗОР',
  'settings.enabled': 'ВКЛ',
  'settings.disabled': 'ВЫКЛ',
  'settings.saved': 'КОНФИГУРАЦИЯ СОХРАНЕНА',
  'settings.saving': 'СОХРАНЕНИЕ...',
  'settings.saveError': 'ОШИБКА СОХРАНЕНИЯ',

  'update.retry': 'ПОВТОРИТЬ',
  'update.playNow': 'ИГРАТЬ',
  'update.filesVerified': 'ФАЙЛЫ ПРОВЕРЕНЫ',
  'update.versionChecked': 'ВЕРСИЯ ПРОВЕРЕНА',
  'update.uptodate': 'АКТУАЛЬНО',
  'update.offline': 'ОФЛАЙН',
  'update.checking': 'ПРОВЕРКА ОБНОВЛЕНИЙ...',
  'update.downloading': 'ЗАГРУЗКА МОДПАКА',

  'main.checking': 'ПРОВЕРКА...',
  'main.updateAvailable': 'ДОСТУПНО ОБНОВЛЕНИЕ',
  'main.launching': 'ЗАПУСК',
  'main.launchFailed': 'ОШИБКА ЗАПУСКА',
  'main.gameStarted': 'ИГРА ЗАПУЩЕНА',
}

export type TKey = keyof typeof ru

export type TFunction = (key: TKey, vars?: Record<string, string | number>) => string

function format(template: string, vars?: Record<string, string | number>) {
  if (!vars) return template
  return template.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
}

export const translate: TFunction = (key, vars) => format(ru[key] ?? key, vars)

type I18n = { lang: Lang; t: TFunction }

export const I18nContext = createContext<I18n>({ lang: 'ru', t: translate })

export const useI18n = () => useContext(I18nContext)
