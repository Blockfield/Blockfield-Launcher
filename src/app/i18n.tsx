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

  'nav.deploy': 'БОЙ',
  'nav.updates': 'ОБНОВЛЕНИЯ',
  'nav.settings': 'НАСТРОЙКИ',
  'shell.rank': 'ЗВАНИЕ · СЕРЖАНТ',
  'shell.logout': 'Выйти',
  'shell.support': 'ПОДДЕРЖКА',
  'shell.network': 'СЕТЬ В НОРМЕ',
  'shell.launcherVersion': 'ЛАУНЧЕР v{v}',
  'shell.ip': 'IP · {ip}',

  'main.operation': 'ОПЕРАЦИЯ',
  'main.active': 'АКТИВНА',
  'main.season': '/ СЕЗОН 01',
  'main.description':
    'Масштабный тактический PvP на спорных территориях. Захватывайте стратегические точки, координируйтесь с отрядом и управляйте бронетехникой, чтобы прорвать вражескую оборону.',
  'main.enterBattlefield': 'НА ПОЛЕ БОЯ',
  'main.modpack': 'МОДПАК',
  'main.upToDate': 'АКТУАЛЕН',
  'main.auth': 'АВТОРИЗАЦИЯ',
  'main.verified': 'ПОДТВЕРЖДЕНА',
  'main.queue': 'ОЧЕРЕДЬ',
  'main.none': 'НЕТ',
  'main.players': 'ИГРОКИ',
  'main.ping': 'ПИНГ',
  'main.region': 'РЕГИОН',
  'main.server': 'СЕРВЕР',
  'main.online': 'ОНЛАЙН',
  'main.serverName': 'BLOCKFIELD · ОСНОВНОЙ',
  'main.briefing': 'БРИФИНГ МИССИИ',
  'main.feat.capture': 'ЗАХВАТ ТОЧЕК',
  'main.feat.captureDesc': 'Динамический контроль точек по нескольким секторам.',
  'main.feat.classes': '6 КЛАССОВ',
  'main.feat.classesDesc': 'Штурмовик, Разведчик, Инженер, Медик, Поддержка, Пилот.',
  'main.feat.vehicles': 'БРОНЕТЕХНИКА',
  'main.feat.vehiclesDesc': 'Танки, БТР, лёгкая разведка и авиатранспорт.',
  'main.feat.battles': 'ТАКТИЧЕСКИЕ БОИ',
  'main.feat.battlesDesc': 'Командные сражения 64 на 64 в постоянном мире.',
  'main.modpackStatus': 'СТАТУС МОДПАКА',
  'main.installed': 'УСТАНОВЛЕН',
  'main.latest': 'ПОСЛЕДНИЙ',
  'main.size': 'РАЗМЕР',
  'main.autoUpdate': 'АВТООБНОВЛЕНИЕ',
  'main.enabled': 'ВКЛ',
  'main.disabled': 'ВЫКЛ',
  'main.fieldReport': 'ПОЛЕВОЙ ОТЧЁТ',
  'main.viewLog': 'ОТКРЫТЬ ЖУРНАЛ ОПЕРАЦИЙ',
  'main.feed.patch.title': '{v} — Баланс техники',
  'main.feed.patch.body':
    'Перезарядка Т-72 уменьшена на 0.4с. Новый разведдрон для класса Скаут. В ротацию добавлены две переработанные карты.',
  'main.feed.event.title': 'Операция «Блэкридж»',
  'main.feed.event.body':
    '48-часовая постоянная кампания стартует в пятницу в 19:00 UTC. Двойной опыт для всех классов.',
  'main.feed.ops.title': 'Обновление античита',
  'main.feed.ops.body':
    'Активирован новый драйвер уровня ядра. Ожидайте меньше ложных срабатываний и более строгий контроль.',
  'main.tag.patch': 'ПАТЧ',
  'main.tag.event': 'ИВЕНТ',
  'main.tag.ops': 'ОПС',

  'update.packageSync': 'СИНХРОНИЗАЦИЯ ПАКЕТА',
  'update.updating': 'ОБНОВЛЕНИЕ МОДПАКА',
  'update.patch': '/ ПАТЧ {v}',
  'update.description':
    'Синхронизация ассетов модпака с основным сервером развёртывания. Не закрывайте лаунчер до завершения операции.',
  'update.mirror': 'ЗЕРКАЛО',
  'update.locked': 'ЗАБЛОКИРОВАНО',
  'update.inProgress': 'ИДЁТ ОБНОВЛЕНИЕ',
  'update.completion': 'ПРОГРЕСС',
  'update.currentFile': 'ТЕКУЩИЙ ФАЙЛ',
  'update.transferred': 'ЗАГРУЖЕНО',
  'update.remaining': 'ОСТАЛОСЬ',
  'update.throughput': 'СКОРОСТЬ',
  'update.status': 'СТАТУС · ОБНОВЛЕНИЕ МОДПАКА (ФАЗА 2 ИЗ 3)',
  'update.integrity': 'ЦЕЛОСТНОСТЬ · SHA256 ПРОВЕРЕН',
  'update.opLog': 'ЖУРНАЛ ОПЕРАЦИИ',
  'update.steps': 'ЭТАПЫ ЗАПУСКА',
  'update.step.verify': 'ПРОВЕРКА МАНИФЕСТА',
  'update.step.setup': 'ПОДГОТОВКА СРЕДЫ',
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
  'settings.runtime': 'СРЕДА',
  'settings.launcher': 'ЛАУНЧЕР',
  'settings.gameDir': 'ПАПКА ИГРЫ',
  'settings.gameDirHint': 'Расположение всех игровых файлов и профилей игрока.',
  'settings.java': 'СРЕДА JAVA',
  'settings.javaHint': 'Путь к Java-файлу, используемому для запуска игры.',
  'settings.ram': 'ВЫДЕЛЕНИЕ RAM',
  'settings.ramHint': '{gb} GB выделено · рекомендуется 6–12 GB',
  'settings.language': 'ЯЗЫК',
  'settings.languageHint': 'Язык интерфейса лаунчера.',
  'settings.autoUpdate': 'АВТООБНОВЛЕНИЕ',
  'settings.autoUpdateHint': 'Автоматически загружать и устанавливать новые версии модпака.',
  'settings.logout': 'ВЫЙТИ ИЗ ПРОФИЛЯ',
  'settings.username': 'НИКНЕЙМ',
  'settings.usernameHint': 'Ник в Minecraft (3-16 букв, цифр или _).',
  'shell.offlineMode': 'ОФЛАЙН-РЕЖИМ',
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
