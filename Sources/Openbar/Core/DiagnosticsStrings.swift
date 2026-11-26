// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Export diagnostics (#230). The Windows and Linux app reads the same table
/// through its extracted catalog (`windows/src/i18n/generated`).
struct DiagnosticsStrings {
    let exportButton: String
    let windowTitle: String
    let intro: String
    let alwaysTitle: String
    let optionalTitle: String
    let systemTitle: String
    let systemCaption: String
    let featuresTitle: String
    let featuresCaption: String
    let agentsTitle: String
    let agentsCaption: String
    let eventsTitle: String
    let eventsCaptionFormat: String
    let hostsTitle: String
    let hostsCaption: String
    let logsTitle: String
    let logsCaptionFormat: String
    let privacyNote: String
    let nothingSent: String
    let cancel: String
    let save: String
    let saveAndReport: String
    let saving: String
    let failedFormat: String
    let settingsTitle: String
    let settingsCaption: String
    let attachTitle: String
    let attachCaption: String
    let reportHint: String
    let statsTitle: String
    let statsCaption: String
    let statsDetails: String
}

extension FeatureStrings {
    static func diagnostics(_ language: AppLanguage) -> DiagnosticsStrings {
        switch language {
        case .enUS: return .enUS
        case .ptBR: return .ptBR
        case .tr: return .tr
        case .ru: return .ru
        case .es: return .es
        case .sk: return .sk
        case .de: return .de
        case .fr: return .fr
        case .it: return .it
        case .ja: return .ja
        case .ko: return .ko
        case .zhHans: return .zhHans
        case .zhTW: return .zhTW
        case .zhHK: return .zhHK
        case .uk: return .uk
        }
    }
}

extension DiagnosticsStrings {
    static let enUS = DiagnosticsStrings(
        exportButton: "Export diagnostics…",
        windowTitle: "Export diagnostics",
        intro: "One file with what we need to understand a bug. Attach it to an issue or send it to us.",
        alwaysTitle: "Always in the file",
        optionalTitle: "Your choice",
        systemTitle: "App and system",
        systemCaption: "Openbar version, update channel, system version, CPU type and displays",
        featuresTitle: "Features",
        featuresCaption: "Which features are on",
        agentsTitle: "Agents and hooks",
        agentsCaption: "Which agents are installed, their versions, the state of their hooks and where their settings are",
        eventsTitle: "Recent session events",
        eventsCaptionFormat: "The last %d events: type, agent, time and state change",
        hostsTitle: "Terminals, accounts and remote hosts",
        hostsCaption: "Names only",
        logsTitle: "Logs",
        logsCaptionFormat: "Openbar’s own logs from the last %d days",
        privacyNote: "Prompts, replies, commands and file contents never go in. Tokens, API keys, email addresses and your home folder are masked.",
        nothingSent: "Nothing is sent anywhere. You decide what to do with the file.",
        cancel: "Cancel",
        save: "Save…",
        saveAndReport: "Save and report…",
        saving: "Saving…",
        failedFormat: "The file could not be saved: %@",
        settingsTitle: "Diagnostics",
        settingsCaption: "Save one file with versions, hook states and logs to attach to a bug report.",
        attachTitle: "Attach a diagnostics file",
        attachCaption: "Reporting a bug first saves the file and shows it in its folder, so you can drag it into the issue.",
        reportHint: "After saving, the bug report form opens. Drag the file into it.",
        statsTitle: "Share anonymous usage stats",
        statsCaption: "Sends a random ID, the app and system versions, and counts of the sessions, terminals and sections you used, once a day. Prompts, files and names are never sent.",
        statsDetails: "What’s sent"
    )

    static let ptBR = DiagnosticsStrings(
        exportButton: "Exportar diagnóstico…",
        windowTitle: "Exportar diagnóstico",
        intro: "Um arquivo com o que precisamos para entender um bug. Anexe a uma issue ou envie para nós.",
        alwaysTitle: "Sempre no arquivo",
        optionalTitle: "Você escolhe",
        systemTitle: "App e sistema",
        systemCaption: "Versão do Openbar, canal de atualização, versão do sistema, tipo de CPU e telas",
        featuresTitle: "Recursos",
        featuresCaption: "Quais recursos estão ativados",
        agentsTitle: "Agentes e hooks",
        agentsCaption: "Quais agentes estão instalados, suas versões, o estado dos hooks e onde ficam os ajustes",
        eventsTitle: "Eventos recentes das sessões",
        eventsCaptionFormat: "Os últimos %d eventos: tipo, agente, hora e mudança de estado",
        hostsTitle: "Terminais, contas e hosts remotos",
        hostsCaption: "Só os nomes",
        logsTitle: "Logs",
        logsCaptionFormat: "Os logs do próprio Openbar dos últimos %d dias",
        privacyNote: "Prompts, respostas, comandos e conteúdo de arquivos nunca entram. Tokens, chaves de API, emails e sua pasta pessoal são mascarados.",
        nothingSent: "Nada é enviado. Você decide o que fazer com o arquivo.",
        cancel: "Cancelar",
        save: "Salvar…",
        saveAndReport: "Salvar e relatar…",
        saving: "Salvando…",
        failedFormat: "Não foi possível salvar o arquivo: %@",
        settingsTitle: "Diagnóstico",
        settingsCaption: "Salve um arquivo com versões, estado dos hooks e logs para anexar a um relato de bug.",
        attachTitle: "Anexar um arquivo de diagnóstico",
        attachCaption: "Ao relatar um bug, o arquivo é salvo antes e mostrado na pasta, para você arrastá-lo para a issue.",
        reportHint: "Depois de salvar, o formulário de bug abre. Arraste o arquivo para ele.",
        statsTitle: "Compartilhar estatísticas de uso anônimas",
        statsCaption: "Uma vez por dia, envia um ID aleatório, as versões do app e do sistema e quantas sessões, terminais e seções você usou. Prompts, arquivos e nomes nunca são enviados.",
        statsDetails: "O que é enviado"
    )

    static let tr = DiagnosticsStrings(
        exportButton: "Tanılamayı dışa aktar…",
        windowTitle: "Tanılamayı dışa aktar",
        intro: "Bir hatayı anlamamız için gerekenleri içeren tek bir dosya. Bir kayda ekleyin ya da bize gönderin.",
        alwaysTitle: "Dosyada her zaman",
        optionalTitle: "Sizin seçiminiz",
        systemTitle: "Uygulama ve sistem",
        systemCaption: "Openbar sürümü, güncelleme kanalı, sistem sürümü, işlemci türü ve ekranlar",
        featuresTitle: "Özellikler",
        featuresCaption: "Hangi özelliklerin açık olduğu",
        agentsTitle: "Ajanlar ve hook’lar",
        agentsCaption: "Hangi ajanların kurulu olduğu, sürümleri, hook’larının durumu ve ayarlarının yeri",
        eventsTitle: "Son oturum olayları",
        eventsCaptionFormat: "Son %d olay: tür, ajan, zaman ve durum değişikliği",
        hostsTitle: "Terminaller, hesaplar ve uzak sunucular",
        hostsCaption: "Yalnızca adlar",
        logsTitle: "Günlükler",
        logsCaptionFormat: "Openbar’ın son %d güne ait kendi günlükleri",
        privacyNote: "İstemler, yanıtlar, komutlar ve dosya içerikleri asla eklenmez. Belirteçler, API anahtarları, e-posta adresleri ve ana klasörünüz gizlenir.",
        nothingSent: "Hiçbir şey gönderilmez. Dosyayla ne yapacağınıza siz karar verirsiniz.",
        cancel: "Vazgeç",
        save: "Kaydet…",
        saveAndReport: "Kaydet ve bildir…",
        saving: "Kaydediliyor…",
        failedFormat: "Dosya kaydedilemedi: %@",
        settingsTitle: "Tanılama",
        settingsCaption: "Hata bildirimine eklemek için sürümleri, hook durumlarını ve günlükleri tek dosyaya kaydedin.",
        attachTitle: "Tanılama dosyası ekle",
        attachCaption: "Hata bildirirken önce dosya kaydedilir ve klasöründe gösterilir; onu kayda sürükleyebilirsiniz.",
        reportHint: "Kaydettikten sonra hata bildirim formu açılır. Dosyayı içine sürükleyin.",
        statsTitle: "Anonim kullanım istatistiklerini paylaş",
        statsCaption: "Günde bir kez rastgele bir kimlik, uygulama ve sistem sürümleri ile kullandığınız oturum, terminal ve bölüm sayılarını gönderir. İstemler, dosyalar ve adlar asla gönderilmez.",
        statsDetails: "Neler gönderilir"
    )

    static let ru = DiagnosticsStrings(
        exportButton: "Экспорт диагностики…",
        windowTitle: "Экспорт диагностики",
        intro: "Один файл со всем, что нужно, чтобы разобраться в ошибке. Приложите его к issue или отправьте нам.",
        alwaysTitle: "Всегда в файле",
        optionalTitle: "На ваш выбор",
        systemTitle: "Приложение и система",
        systemCaption: "Версия Openbar, канал обновлений, версия системы, тип процессора и дисплеи",
        featuresTitle: "Функции",
        featuresCaption: "Какие функции включены",
        agentsTitle: "Агенты и хуки",
        agentsCaption: "Какие агенты установлены, их версии, состояние хуков и где лежат их настройки",
        eventsTitle: "Недавние события сессий",
        eventsCaptionFormat: "Последние %d событий: тип, агент, время и смена состояния",
        hostsTitle: "Терминалы, аккаунты и удалённые хосты",
        hostsCaption: "Только названия",
        logsTitle: "Журналы",
        logsCaptionFormat: "Собственные журналы Openbar за последние %d дн.",
        privacyNote: "Запросы, ответы, команды и содержимое файлов никогда не попадают в файл. Токены, ключи API, адреса почты и домашняя папка скрыты.",
        nothingSent: "Ничего никуда не отправляется. Вы сами решаете, что делать с файлом.",
        cancel: "Отменить",
        save: "Сохранить…",
        saveAndReport: "Сохранить и сообщить…",
        saving: "Сохранение…",
        failedFormat: "Не удалось сохранить файл: %@",
        settingsTitle: "Диагностика",
        settingsCaption: "Сохраните один файл с версиями, состоянием хуков и журналами, чтобы приложить его к отчёту об ошибке.",
        attachTitle: "Приложить файл диагностики",
        attachCaption: "При отчёте об ошибке файл сначала сохраняется и показывается в папке, чтобы перетащить его в issue.",
        reportHint: "После сохранения откроется форма отчёта. Перетащите в неё файл.",
        statsTitle: "Отправлять анонимную статистику",
        statsCaption: "Раз в день отправляет случайный ID, версии приложения и системы и число использованных сеансов, терминалов и разделов. Запросы, файлы и имена не отправляются никогда.",
        statsDetails: "Что отправляется"
    )

    static let es = DiagnosticsStrings(
        exportButton: "Exportar diagnóstico…",
        windowTitle: "Exportar diagnóstico",
        intro: "Un archivo con lo que necesitamos para entender un error. Adjúntalo a una incidencia o envíanoslo.",
        alwaysTitle: "Siempre en el archivo",
        optionalTitle: "Tú eliges",
        systemTitle: "App y sistema",
        systemCaption: "Versión de Openbar, canal de actualizaciones, versión del sistema, tipo de CPU y pantallas",
        featuresTitle: "Funciones",
        featuresCaption: "Qué funciones están activadas",
        agentsTitle: "Agentes y hooks",
        agentsCaption: "Qué agentes están instalados, sus versiones, el estado de sus hooks y dónde están sus ajustes",
        eventsTitle: "Eventos recientes de sesiones",
        eventsCaptionFormat: "Los últimos %d eventos: tipo, agente, hora y cambio de estado",
        hostsTitle: "Terminales, cuentas y hosts remotos",
        hostsCaption: "Solo los nombres",
        logsTitle: "Registros",
        logsCaptionFormat: "Los registros propios de Openbar de los últimos %d días",
        privacyNote: "Nunca entran prompts, respuestas, comandos ni contenido de archivos. Se ocultan tokens, claves de API, correos y tu carpeta de inicio.",
        nothingSent: "No se envía nada. Tú decides qué hacer con el archivo.",
        cancel: "Cancelar",
        save: "Guardar…",
        saveAndReport: "Guardar e informar…",
        saving: "Guardando…",
        failedFormat: "No se pudo guardar el archivo: %@",
        settingsTitle: "Diagnóstico",
        settingsCaption: "Guarda un archivo con versiones, estado de los hooks y registros para adjuntarlo a un informe de error.",
        attachTitle: "Adjuntar un archivo de diagnóstico",
        attachCaption: "Al informar de un error, primero se guarda el archivo y se muestra en su carpeta para que lo arrastres a la incidencia.",
        reportHint: "Al guardar se abre el formulario del informe. Arrastra el archivo dentro.",
        statsTitle: "Compartir estadísticas de uso anónimas",
        statsCaption: "Una vez al día envía un ID aleatorio, las versiones de la app y del sistema, y cuántas sesiones, terminales y secciones usaste. Nunca envía instrucciones, archivos ni nombres.",
        statsDetails: "Qué se envía"
    )

    static let sk = DiagnosticsStrings(
        exportButton: "Exportovať diagnostiku…",
        windowTitle: "Exportovať diagnostiku",
        intro: "Jeden súbor so všetkým, čo potrebujeme na pochopenie chyby. Priložte ho k issue alebo nám ho pošlite.",
        alwaysTitle: "Vždy v súbore",
        optionalTitle: "Podľa vás",
        systemTitle: "Aplikácia a systém",
        systemCaption: "Verzia Openbaru, kanál aktualizácií, verzia systému, typ procesora a displeje",
        featuresTitle: "Funkcie",
        featuresCaption: "Ktoré funkcie sú zapnuté",
        agentsTitle: "Agenti a hooky",
        agentsCaption: "Ktorí agenti sú nainštalovaní, ich verzie, stav ich hookov a kde sú ich nastavenia",
        eventsTitle: "Nedávne udalosti relácií",
        eventsCaptionFormat: "Posledných %d udalostí: typ, agent, čas a zmena stavu",
        hostsTitle: "Terminály, účty a vzdialení hostitelia",
        hostsCaption: "Iba názvy",
        logsTitle: "Záznamy",
        logsCaptionFormat: "Vlastné záznamy Openbaru za posledné %d dni",
        privacyNote: "Výzvy, odpovede, príkazy a obsah súborov sa nikdy nepridajú. Tokeny, kľúče API, e-mailové adresy a váš domovský priečinok sa skryjú.",
        nothingSent: "Nič sa nikam neposiela. O súbore rozhodujete vy.",
        cancel: "Zrušiť",
        save: "Uložiť…",
        saveAndReport: "Uložiť a nahlásiť…",
        saving: "Ukladá sa…",
        failedFormat: "Súbor sa nepodarilo uložiť: %@",
        settingsTitle: "Diagnostika",
        settingsCaption: "Uložte jeden súbor s verziami, stavom hookov a záznamami a priložte ho k hláseniu chyby.",
        attachTitle: "Priložiť súbor diagnostiky",
        attachCaption: "Pri hlásení chyby sa súbor najprv uloží a zobrazí v priečinku, aby ste ho mohli pretiahnuť do issue.",
        reportHint: "Po uložení sa otvorí formulár hlásenia. Pretiahnite doň súbor.",
        statsTitle: "Zdieľať anonymné štatistiky používania",
        statsCaption: "Raz denne odošle náhodné ID, verziu aplikácie a systému a počty použitých relácií, terminálov a sekcií. Prompty, súbory ani názvy sa nikdy neodosielajú.",
        statsDetails: "Čo sa odosiela"
    )

    static let de = DiagnosticsStrings(
        exportButton: "Diagnose exportieren…",
        windowTitle: "Diagnose exportieren",
        intro: "Eine Datei mit allem, was wir brauchen, um einen Fehler zu verstehen. Hänge sie an ein Issue an oder schick sie uns.",
        alwaysTitle: "Immer in der Datei",
        optionalTitle: "Deine Wahl",
        systemTitle: "App und System",
        systemCaption: "Openbar-Version, Update-Kanal, Systemversion, CPU-Typ und Bildschirme",
        featuresTitle: "Funktionen",
        featuresCaption: "Welche Funktionen eingeschaltet sind",
        agentsTitle: "Agenten und Hooks",
        agentsCaption: "Welche Agenten installiert sind, ihre Versionen, der Zustand ihrer Hooks und wo ihre Einstellungen liegen",
        eventsTitle: "Letzte Sitzungsereignisse",
        eventsCaptionFormat: "Die letzten %d Ereignisse: Art, Agent, Zeit und Zustandswechsel",
        hostsTitle: "Terminals, Konten und entfernte Hosts",
        hostsCaption: "Nur die Namen",
        logsTitle: "Protokolle",
        logsCaptionFormat: "Openbars eigene Protokolle der letzten %d Tage",
        privacyNote: "Prompts, Antworten, Befehle und Dateiinhalte kommen nie hinein. Tokens, API-Schlüssel, E-Mail-Adressen und dein Benutzerordner werden maskiert.",
        nothingSent: "Es wird nichts gesendet. Du entscheidest, was mit der Datei passiert.",
        cancel: "Abbrechen",
        save: "Sichern…",
        saveAndReport: "Sichern und melden…",
        saving: "Wird gesichert…",
        failedFormat: "Die Datei konnte nicht gesichert werden: %@",
        settingsTitle: "Diagnose",
        settingsCaption: "Sichere eine Datei mit Versionen, Hook-Zuständen und Protokollen für einen Fehlerbericht.",
        attachTitle: "Diagnosedatei anhängen",
        attachCaption: "Beim Melden eines Fehlers wird die Datei zuerst gesichert und in ihrem Ordner gezeigt, damit du sie ins Issue ziehen kannst.",
        reportHint: "Nach dem Sichern öffnet sich das Formular für den Fehlerbericht. Zieh die Datei hinein.",
        statsTitle: "Anonyme Nutzungsstatistiken teilen",
        statsCaption: "Sendet einmal am Tag eine zufällige ID, die App- und Systemversion und wie viele Sitzungen, Terminals und Bereiche du genutzt hast. Prompts, Dateien und Namen werden nie gesendet.",
        statsDetails: "Was gesendet wird"
    )

    static let fr = DiagnosticsStrings(
        exportButton: "Exporter le diagnostic…",
        windowTitle: "Exporter le diagnostic",
        intro: "Un seul fichier avec ce qu’il nous faut pour comprendre un bug. Joignez-le à une issue ou envoyez-le-nous.",
        alwaysTitle: "Toujours dans le fichier",
        optionalTitle: "À vous de choisir",
        systemTitle: "App et système",
        systemCaption: "Version d’Openbar, canal de mise à jour, version du système, type de processeur et écrans",
        featuresTitle: "Fonctions",
        featuresCaption: "Les fonctions activées",
        agentsTitle: "Agents et hooks",
        agentsCaption: "Les agents installés, leurs versions, l’état de leurs hooks et l’emplacement de leurs réglages",
        eventsTitle: "Événements récents des sessions",
        eventsCaptionFormat: "Les %d derniers événements : type, agent, heure et changement d’état",
        hostsTitle: "Terminaux, comptes et hôtes distants",
        hostsCaption: "Les noms seulement",
        logsTitle: "Journaux",
        logsCaptionFormat: "Les journaux d’Openbar des %d derniers jours",
        privacyNote: "Les prompts, réponses, commandes et contenus de fichiers n’y figurent jamais. Les jetons, clés d’API, adresses e-mail et votre dossier personnel sont masqués.",
        nothingSent: "Rien n’est envoyé. Vous décidez quoi faire du fichier.",
        cancel: "Annuler",
        save: "Enregistrer…",
        saveAndReport: "Enregistrer et signaler…",
        saving: "Enregistrement…",
        failedFormat: "Impossible d’enregistrer le fichier : %@",
        settingsTitle: "Diagnostic",
        settingsCaption: "Enregistrez un fichier avec les versions, l’état des hooks et les journaux pour un rapport de bug.",
        attachTitle: "Joindre un fichier de diagnostic",
        attachCaption: "Signaler un bug enregistre d’abord le fichier et l’affiche dans son dossier, pour le glisser dans l’issue.",
        reportHint: "Après l’enregistrement, le formulaire de bug s’ouvre. Glissez-y le fichier.",
        statsTitle: "Partager des statistiques d’usage anonymes",
        statsCaption: "Une fois par jour, envoie un identifiant aléatoire, les versions de l’app et du système, et le nombre de sessions, terminaux et sections utilisés. Jamais de requêtes, de fichiers ni de noms.",
        statsDetails: "Ce qui est envoyé"
    )

    static let it = DiagnosticsStrings(
        exportButton: "Esporta diagnostica…",
        windowTitle: "Esporta diagnostica",
        intro: "Un solo file con ciò che ci serve per capire un bug. Allegalo a una issue o mandacelo.",
        alwaysTitle: "Sempre nel file",
        optionalTitle: "A tua scelta",
        systemTitle: "App e sistema",
        systemCaption: "Versione di Openbar, canale di aggiornamento, versione del sistema, tipo di CPU e schermi",
        featuresTitle: "Funzioni",
        featuresCaption: "Quali funzioni sono attive",
        agentsTitle: "Agenti e hook",
        agentsCaption: "Quali agenti sono installati, le versioni, lo stato dei loro hook e dove sono le loro impostazioni",
        eventsTitle: "Eventi recenti delle sessioni",
        eventsCaptionFormat: "Gli ultimi %d eventi: tipo, agente, ora e cambio di stato",
        hostsTitle: "Terminali, account e host remoti",
        hostsCaption: "Solo i nomi",
        logsTitle: "Log",
        logsCaptionFormat: "I log di Openbar degli ultimi %d giorni",
        privacyNote: "Prompt, risposte, comandi e contenuti dei file non entrano mai. Token, chiavi API, indirizzi email e la cartella Inizio vengono mascherati.",
        nothingSent: "Non viene inviato nulla. Decidi tu cosa fare del file.",
        cancel: "Annulla",
        save: "Salva…",
        saveAndReport: "Salva e segnala…",
        saving: "Salvataggio…",
        failedFormat: "Impossibile salvare il file: %@",
        settingsTitle: "Diagnostica",
        settingsCaption: "Salva un file con versioni, stato degli hook e log da allegare a una segnalazione di bug.",
        attachTitle: "Allega un file di diagnostica",
        attachCaption: "Segnalando un bug, il file viene prima salvato e mostrato nella sua cartella, così puoi trascinarlo nella issue.",
        reportHint: "Dopo il salvataggio si apre il modulo della segnalazione. Trascinaci il file.",
        statsTitle: "Condividi statistiche d’uso anonime",
        statsCaption: "Una volta al giorno invia un ID casuale, le versioni dell’app e del sistema e quante sessioni, terminali e sezioni hai usato. Prompt, file e nomi non vengono mai inviati.",
        statsDetails: "Cosa viene inviato"
    )

    static let ja = DiagnosticsStrings(
        exportButton: "診断情報を書き出す…",
        windowTitle: "診断情報を書き出す",
        intro: "不具合を理解するのに必要な情報を1つのファイルにまとめます。Issueに添付するか、私たちに送ってください。",
        alwaysTitle: "常に含まれるもの",
        optionalTitle: "選べるもの",
        systemTitle: "アプリとシステム",
        systemCaption: "Openbarのバージョン、アップデートチャンネル、システムのバージョン、CPUの種類、ディスプレイ",
        featuresTitle: "機能",
        featuresCaption: "オンになっている機能",
        agentsTitle: "エージェントとフック",
        agentsCaption: "インストール済みのエージェント、そのバージョン、フックの状態、設定の場所",
        eventsTitle: "最近のセッションイベント",
        eventsCaptionFormat: "直近%d件のイベント: 種類、エージェント、時刻、状態の変化",
        hostsTitle: "ターミナル、アカウント、リモートホスト",
        hostsCaption: "名前のみ",
        logsTitle: "ログ",
        logsCaptionFormat: "過去%d日分のOpenbar自身のログ",
        privacyNote: "プロンプト、返答、コマンド、ファイルの内容は含まれません。トークン、APIキー、メールアドレス、ホームフォルダは伏せられます。",
        nothingSent: "どこにも送信されません。ファイルをどうするかはあなたが決めます。",
        cancel: "キャンセル",
        save: "保存…",
        saveAndReport: "保存して報告…",
        saving: "保存中…",
        failedFormat: "ファイルを保存できませんでした: %@",
        settingsTitle: "診断",
        settingsCaption: "バージョン、フックの状態、ログを1つのファイルに保存して、不具合報告に添付できます。",
        attachTitle: "診断ファイルを添付",
        attachCaption: "不具合を報告するとき、先にファイルを保存してフォルダで表示します。Issueにドラッグしてください。",
        reportHint: "保存すると不具合報告のフォームが開きます。ファイルをそこへドラッグしてください。",
        statsTitle: "匿名の使用統計を共有",
        statsCaption: "1日に1回、ランダムなID、アプリとシステムのバージョン、使ったセッション・ターミナル・セクションの数を送信します。プロンプト、ファイル、名前は送信しません。",
        statsDetails: "送信される内容"
    )

    static let ko = DiagnosticsStrings(
        exportButton: "진단 정보 내보내기…",
        windowTitle: "진단 정보 내보내기",
        intro: "버그를 이해하는 데 필요한 정보를 파일 하나에 담습니다. 이슈에 첨부하거나 저희에게 보내 주세요.",
        alwaysTitle: "항상 포함",
        optionalTitle: "선택 항목",
        systemTitle: "앱과 시스템",
        systemCaption: "Openbar 버전, 업데이트 채널, 시스템 버전, CPU 종류와 디스플레이",
        featuresTitle: "기능",
        featuresCaption: "켜져 있는 기능",
        agentsTitle: "에이전트와 훅",
        agentsCaption: "설치된 에이전트, 버전, 훅 상태와 설정 위치",
        eventsTitle: "최근 세션 이벤트",
        eventsCaptionFormat: "최근 이벤트 %d개: 종류, 에이전트, 시간, 상태 변화",
        hostsTitle: "터미널, 계정, 원격 호스트",
        hostsCaption: "이름만",
        logsTitle: "로그",
        logsCaptionFormat: "최근 %d일간의 Openbar 자체 로그",
        privacyNote: "프롬프트, 답변, 명령어, 파일 내용은 절대 들어가지 않습니다. 토큰, API 키, 이메일 주소, 홈 폴더는 가려집니다.",
        nothingSent: "아무것도 전송되지 않습니다. 파일을 어떻게 할지는 직접 정합니다.",
        cancel: "취소",
        save: "저장…",
        saveAndReport: "저장하고 보고…",
        saving: "저장 중…",
        failedFormat: "파일을 저장하지 못했습니다: %@",
        settingsTitle: "진단",
        settingsCaption: "버전, 훅 상태, 로그를 파일 하나로 저장해 버그 보고에 첨부합니다.",
        attachTitle: "진단 파일 첨부",
        attachCaption: "버그를 보고할 때 먼저 파일을 저장하고 폴더에서 보여 줍니다. 이슈로 끌어다 놓으세요.",
        reportHint: "저장하면 버그 보고 양식이 열립니다. 파일을 그곳으로 끌어다 놓으세요.",
        statsTitle: "익명 사용 통계 공유",
        statsCaption: "하루에 한 번 무작위 ID, 앱과 시스템 버전, 사용한 세션·터미널·섹션 수를 보냅니다. 프롬프트, 파일, 이름은 보내지 않습니다.",
        statsDetails: "보내는 정보"
    )

    static let uk = DiagnosticsStrings(
        exportButton: "Експорт діагностики…",
        windowTitle: "Експорт діагностики",
        intro: "Один файл з усім, що потрібно, щоб розібратися з помилкою. Додайте його до issue або надішліть нам.",
        alwaysTitle: "Завжди у файлі",
        optionalTitle: "На ваш вибір",
        systemTitle: "Застосунок і система",
        systemCaption: "Версія Openbar, канал оновлень, версія системи, тип процесора та дисплеї",
        featuresTitle: "Функції",
        featuresCaption: "Які функції увімкнено",
        agentsTitle: "Агенти та хуки",
        agentsCaption: "Які агенти встановлено, їхні версії, стан хуків і де лежать їхні налаштування",
        eventsTitle: "Нещодавні події сесій",
        eventsCaptionFormat: "Останні %d подій: тип, агент, час і зміна стану",
        hostsTitle: "Термінали, облікові записи та віддалені хости",
        hostsCaption: "Лише назви",
        logsTitle: "Журнали",
        logsCaptionFormat: "Власні журнали Openbar за останні %d дн.",
        privacyNote: "Запити, відповіді, команди та вміст файлів ніколи не потрапляють у файл. Токени, ключі API, адреси пошти та домашню папку приховано.",
        nothingSent: "Нічого нікуди не надсилається. Ви самі вирішуєте, що робити з файлом.",
        cancel: "Скасувати",
        save: "Зберегти…",
        saveAndReport: "Зберегти й повідомити…",
        saving: "Збереження…",
        failedFormat: "Не вдалося зберегти файл: %@",
        settingsTitle: "Діагностика",
        settingsCaption: "Збережіть один файл з версіями, станом хуків і журналами, щоб додати його до звіту про помилку.",
        attachTitle: "Додати файл діагностики",
        attachCaption: "Під час звіту про помилку файл спершу зберігається й показується в папці, щоб перетягнути його в issue.",
        reportHint: "Після збереження відкриється форма звіту. Перетягніть у неї файл.",
        statsTitle: "Надсилати анонімну статистику",
        statsCaption: "Раз на день надсилає випадковий ID, версії програми й системи та кількість використаних сеансів, терміналів і розділів. Запити, файли й імена не надсилаються ніколи.",
        statsDetails: "Що надсилається"
    )

    static let zhHans = DiagnosticsStrings(
        exportButton: "导出诊断信息…",
        windowTitle: "导出诊断信息",
        intro: "把我们了解问题所需的信息放进一个文件。附到 issue 中，或发给我们。",
        alwaysTitle: "始终包含",
        optionalTitle: "由你选择",
        systemTitle: "应用和系统",
        systemCaption: "Openbar 版本、更新渠道、系统版本、CPU 类型和显示器",
        featuresTitle: "功能",
        featuresCaption: "哪些功能已打开",
        agentsTitle: "智能体和钩子",
        agentsCaption: "安装了哪些智能体、它们的版本、钩子状态以及设置所在位置",
        eventsTitle: "最近的会话事件",
        eventsCaptionFormat: "最近 %d 个事件：类型、智能体、时间和状态变化",
        hostsTitle: "终端、账户和远程主机",
        hostsCaption: "仅名称",
        logsTitle: "日志",
        logsCaptionFormat: "Openbar 最近 %d 天的自身日志",
        privacyNote: "绝不包含提示词、回复、命令和文件内容。令牌、API 密钥、电子邮件地址和你的个人文件夹都会被遮盖。",
        nothingSent: "不会发送任何内容。文件如何处理由你决定。",
        cancel: "取消",
        save: "存储…",
        saveAndReport: "存储并报告…",
        saving: "正在存储…",
        failedFormat: "无法存储文件：%@",
        settingsTitle: "诊断",
        settingsCaption: "把版本、钩子状态和日志存成一个文件，附到问题报告中。",
        attachTitle: "附加诊断文件",
        attachCaption: "报告问题时会先存储文件并在其文件夹中显示，方便你把它拖进 issue。",
        reportHint: "存储后会打开问题报告表单。把文件拖进去即可。",
        statsTitle: "共享匿名使用统计",
        statsCaption: "每天一次，发送一个随机 ID、App 与系统版本，以及你使用的会话、终端和栏目数量。绝不发送提示、文件或名称。",
        statsDetails: "发送的内容"
    )

    static let zhTW = DiagnosticsStrings(
        exportButton: "匯出診斷資訊…",
        windowTitle: "匯出診斷資訊",
        intro: "把我們了解問題所需的資訊放進一個檔案。附加到 issue，或傳給我們。",
        alwaysTitle: "一律包含",
        optionalTitle: "由你選擇",
        systemTitle: "App 與系統",
        systemCaption: "Openbar 版本、更新頻道、系統版本、CPU 類型與顯示器",
        featuresTitle: "功能",
        featuresCaption: "哪些功能已開啟",
        agentsTitle: "代理程式與掛鉤",
        agentsCaption: "安裝了哪些代理程式、它們的版本、掛鉤狀態與設定所在位置",
        eventsTitle: "最近的工作階段事件",
        eventsCaptionFormat: "最近 %d 個事件：類型、代理程式、時間與狀態變化",
        hostsTitle: "終端機、帳號與遠端主機",
        hostsCaption: "僅名稱",
        logsTitle: "記錄",
        logsCaptionFormat: "Openbar 最近 %d 天的自身記錄",
        privacyNote: "絕不包含提示、回覆、指令與檔案內容。權杖、API 金鑰、電子郵件地址與你的個人資料夾都會被遮蓋。",
        nothingSent: "不會傳送任何內容。檔案如何處理由你決定。",
        cancel: "取消",
        save: "儲存…",
        saveAndReport: "儲存並回報…",
        saving: "正在儲存…",
        failedFormat: "無法儲存檔案：%@",
        settingsTitle: "診斷",
        settingsCaption: "把版本、掛鉤狀態與記錄存成一個檔案，附加到問題回報中。",
        attachTitle: "附加診斷檔案",
        attachCaption: "回報問題時會先儲存檔案並在其資料夾中顯示，方便你把它拖進 issue。",
        reportHint: "儲存後會開啟問題回報表單。把檔案拖進去即可。",
        statsTitle: "分享匿名使用統計",
        statsCaption: "每天一次，傳送一個隨機 ID、App 與系統版本，以及你使用的工作階段、終端機和區塊數量。絕不傳送提示、檔案或名稱。",
        statsDetails: "傳送的內容"
    )

    static let zhHK = DiagnosticsStrings(
        exportButton: "匯出診斷資料…",
        windowTitle: "匯出診斷資料",
        intro: "把我們了解問題所需的資料放進一個檔案。附加到 issue，或傳給我們。",
        alwaysTitle: "一律包括",
        optionalTitle: "由你選擇",
        systemTitle: "App 與系統",
        systemCaption: "Openbar 版本、更新頻道、系統版本、CPU 類型與顯示器",
        featuresTitle: "功能",
        featuresCaption: "哪些功能已開啟",
        agentsTitle: "代理程式與掛鈎",
        agentsCaption: "安裝了哪些代理程式、它們的版本、掛鈎狀態與設定所在位置",
        eventsTitle: "最近的工作階段事件",
        eventsCaptionFormat: "最近 %d 個事件：類型、代理程式、時間與狀態變化",
        hostsTitle: "終端機、帳戶與遠端主機",
        hostsCaption: "只有名稱",
        logsTitle: "記錄",
        logsCaptionFormat: "Openbar 最近 %d 日的自身記錄",
        privacyNote: "絕不包括提示、回覆、指令與檔案內容。權杖、API 密鑰、電郵地址與你的個人資料夾都會被遮蓋。",
        nothingSent: "不會傳送任何內容。檔案如何處理由你決定。",
        cancel: "取消",
        save: "儲存…",
        saveAndReport: "儲存並報告…",
        saving: "正在儲存…",
        failedFormat: "無法儲存檔案：%@",
        settingsTitle: "診斷",
        settingsCaption: "把版本、掛鈎狀態與記錄存成一個檔案，附加到問題報告中。",
        attachTitle: "附加診斷檔案",
        attachCaption: "報告問題時會先儲存檔案並在其資料夾中顯示，方便你把它拖進 issue。",
        reportHint: "儲存後會開啟問題報告表單。把檔案拖進去即可。",
        statsTitle: "分享匿名使用統計",
        statsCaption: "每日一次，傳送一個隨機 ID、App 與系統版本，以及你使用的工作階段、終端機和區塊數量。絕不傳送提示、檔案或名稱。",
        statsDetails: "傳送的內容"
    )
}
