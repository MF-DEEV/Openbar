// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Voice replies in the Sessions section. Unlike the rest of the section,
/// which speaks English for now, these follow the app's language: the
/// recognizer listens in it, and its problems are the person's to fix.
struct NotchVoiceReplyStrings {
    let settingTitle: String
    let settingHint: String
    let language: String
    let followAppFormat: String
    let notOnDeviceTag: String
    let shortcut: String
    let shortcutHint: String
    let sendOnStop: String
    let sendOnStopHint: String
    let start: String
    let stop: String
    let listening: String
    let escCancels: String
    let noMicrophone: String
    let microphoneDenied: String
    let speechDenied: String
    let notOnDeviceFormat: String
    let dictationOff: String
    let recognizerFailed: String
    let openSettings: String

    func followApp(_ language: String) -> String { String(format: followAppFormat, language) }
    func notOnDevice(_ language: String) -> String { String(format: notOnDeviceFormat, language) }

    static func forLanguage(_ language: AppLanguage) -> NotchVoiceReplyStrings {
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
        case .uk: return .uk
        case .zhHans: return .zhHans
        case .zhTW: return .zhTW
        case .zhHK: return .zhHK
        }
    }
}

extension NotchVoiceReplyStrings {
    static let enUS = NotchVoiceReplyStrings(
        settingTitle: "Voice replies",
        settingHint: "A microphone in a session’s reply field: speak, and the words fill in as you go. Speech is recognized on this Mac only, never on a server.",
        language: "Language",
        followAppFormat: "Same as the app (%@)",
        notOnDeviceTag: "not on this Mac",
        shortcut: "Hold to talk",
        shortcutHint: "Hold the keys and speak to the session on screen, or to the one last at work. Let go to stop.",
        sendOnStop: "Send when I stop talking",
        sendOnStopHint: "The words go to the session as soon as recording ends, without ⌘↩.",
        start: "Speak a reply",
        stop: "Stop recording",
        listening: "Listening…",
        escCancels: "Esc cancels",
        noMicrophone: "No microphone is connected.",
        microphoneDenied: "Openbar isn’t allowed to use the microphone.",
        speechDenied: "Openbar isn’t allowed to recognize speech.",
        notOnDeviceFormat: "%@ can’t be recognized on this Mac. Add it under Dictation in Keyboard settings.",
        dictationOff: "Turn on Dictation in Keyboard settings; Apple recognizes speech on this Mac only with it on.",
        recognizerFailed: "Speech couldn’t be recognized. Try again.",
        openSettings: "Open Settings"
    )

    static let ptBR = NotchVoiceReplyStrings(
        settingTitle: "Respostas por voz",
        settingHint: "Um microfone no campo de resposta da sessão: fale e as palavras aparecem enquanto você fala. A fala é reconhecida só neste Mac, nunca em um servidor.",
        language: "Idioma",
        followAppFormat: "Igual ao app (%@)",
        notOnDeviceTag: "indisponível neste Mac",
        shortcut: "Segurar para falar",
        shortcutHint: "Segure as teclas e fale com a sessão na tela ou com a última que trabalhou. Solte para parar.",
        sendOnStop: "Enviar quando eu parar de falar",
        sendOnStopHint: "As palavras vão para a sessão assim que a gravação termina, sem ⌘↩.",
        start: "Responder por voz",
        stop: "Parar gravação",
        listening: "Ouvindo…",
        escCancels: "Esc cancela",
        noMicrophone: "Nenhum microfone conectado.",
        microphoneDenied: "O Openbar não tem permissão para usar o microfone.",
        speechDenied: "O Openbar não tem permissão para reconhecer fala.",
        notOnDeviceFormat: "%@ não pode ser reconhecido neste Mac. Adicione-o em Ditado nos ajustes de Teclado.",
        dictationOff: "Ative o Ditado nos ajustes de Teclado; a Apple só reconhece fala neste Mac com ele ativado.",
        recognizerFailed: "Não foi possível reconhecer a fala. Tente de novo.",
        openSettings: "Abrir Ajustes"
    )

    static let tr = NotchVoiceReplyStrings(
        settingTitle: "Sesli yanıtlar",
        settingHint: "Oturumun yanıt alanında bir mikrofon: konuşun, sözcükler siz konuştukça yazılsın. Konuşma yalnızca bu Mac’te tanınır, hiçbir zaman bir sunucuda değil.",
        language: "Dil",
        followAppFormat: "Uygulamayla aynı (%@)",
        notOnDeviceTag: "bu Mac’te yok",
        shortcut: "Basılı tut ve konuş",
        shortcutHint: "Tuşları basılı tutup ekrandaki oturumla ya da en son çalışanla konuşun. Durdurmak için bırakın.",
        sendOnStop: "Konuşmayı bitirince gönder",
        sendOnStopHint: "Kayıt biter bitmez sözcükler ⌘↩ olmadan oturuma gider.",
        start: "Sesle yanıtla",
        stop: "Kaydı durdur",
        listening: "Dinleniyor…",
        escCancels: "Esc iptal eder",
        noMicrophone: "Bağlı mikrofon yok.",
        microphoneDenied: "Openbar’in mikrofonu kullanma izni yok.",
        speechDenied: "Openbar’in konuşma tanıma izni yok.",
        notOnDeviceFormat: "%@ bu Mac’te tanınamıyor. Klavye ayarlarında Dikte altından ekleyin.",
        dictationOff: "Klavye ayarlarında Dikte’yi açın; Apple konuşmayı bu Mac’te yalnızca Dikte açıkken tanır.",
        recognizerFailed: "Konuşma tanınamadı. Yeniden deneyin.",
        openSettings: "Ayarları Aç"
    )

    static let ru = NotchVoiceReplyStrings(
        settingTitle: "Голосовые ответы",
        settingHint: "Микрофон в поле ответа сессии: говорите, и слова появляются по ходу речи. Речь распознаётся только на этом Mac, никогда на сервере.",
        language: "Язык",
        followAppFormat: "Как в приложении (%@)",
        notOnDeviceTag: "нет на этом Mac",
        shortcut: "Говорить, удерживая",
        shortcutHint: "Удерживайте клавиши и говорите с сессией на экране или с той, что работала последней. Отпустите, чтобы остановить.",
        sendOnStop: "Отправлять, когда я замолкаю",
        sendOnStopHint: "Слова уходят в сессию, как только запись закончится, без ⌘↩.",
        start: "Ответить голосом",
        stop: "Остановить запись",
        listening: "Слушаю…",
        escCancels: "Esc — отмена",
        noMicrophone: "Микрофон не подключён.",
        microphoneDenied: "Openbar не разрешено использовать микрофон.",
        speechDenied: "Openbar не разрешено распознавать речь.",
        notOnDeviceFormat: "%@ нельзя распознать на этом Mac. Добавьте язык в разделе «Диктовка» настроек клавиатуры.",
        dictationOff: "Включите диктовку в настройках клавиатуры: без неё Apple не распознаёт речь на этом Mac.",
        recognizerFailed: "Не удалось распознать речь. Попробуйте ещё раз.",
        openSettings: "Открыть настройки"
    )

    static let es = NotchVoiceReplyStrings(
        settingTitle: "Respuestas por voz",
        settingHint: "Un micrófono en el campo de respuesta de la sesión: habla y las palabras aparecen mientras hablas. La voz se reconoce solo en este Mac, nunca en un servidor.",
        language: "Idioma",
        followAppFormat: "Como la app (%@)",
        notOnDeviceTag: "no disponible en este Mac",
        shortcut: "Mantener para hablar",
        shortcutHint: "Mantén las teclas y habla con la sesión en pantalla o con la última que trabajó. Suelta para parar.",
        sendOnStop: "Enviar cuando deje de hablar",
        sendOnStopHint: "Las palabras van a la sesión en cuanto termina la grabación, sin ⌘↩.",
        start: "Responder por voz",
        stop: "Detener grabación",
        listening: "Escuchando…",
        escCancels: "Esc cancela",
        noMicrophone: "No hay ningún micrófono conectado.",
        microphoneDenied: "Openbar no tiene permiso para usar el micrófono.",
        speechDenied: "Openbar no tiene permiso para reconocer la voz.",
        notOnDeviceFormat: "%@ no se puede reconocer en este Mac. Añádelo en Dictado, en los ajustes de Teclado.",
        dictationOff: "Activa Dictado en los ajustes de Teclado; Apple solo reconoce la voz en este Mac con él activado.",
        recognizerFailed: "No se pudo reconocer la voz. Inténtalo de nuevo.",
        openSettings: "Abrir Ajustes"
    )

    static let sk = NotchVoiceReplyStrings(
        settingTitle: "Hlasové odpovede",
        settingHint: "Mikrofón v poli odpovede relácie: hovorte a slová sa dopĺňajú priebežne. Reč sa rozpoznáva len na tomto Macu, nikdy na serveri.",
        language: "Jazyk",
        followAppFormat: "Ako aplikácia (%@)",
        notOnDeviceTag: "nie je na tomto Macu",
        shortcut: "Podržať a hovoriť",
        shortcutHint: "Podržte klávesy a hovorte k relácii na obrazovke alebo k tej, ktorá pracovala naposledy. Pustením zastavíte.",
        sendOnStop: "Odoslať, keď prestanem hovoriť",
        sendOnStopHint: "Slová idú do relácie hneď po skončení nahrávania, bez ⌘↩.",
        start: "Odpovedať hlasom",
        stop: "Zastaviť nahrávanie",
        listening: "Počúvam…",
        escCancels: "Esc zruší",
        noMicrophone: "Nie je pripojený žiadny mikrofón.",
        microphoneDenied: "Openbar nemá povolenie používať mikrofón.",
        speechDenied: "Openbar nemá povolenie rozpoznávať reč.",
        notOnDeviceFormat: "%@ sa na tomto Macu nedá rozpoznať. Pridajte ho v Diktovaní v nastaveniach klávesnice.",
        dictationOff: "Zapnite Diktovanie v nastaveniach klávesnice; bez neho Apple reč na tomto Macu nerozpozná.",
        recognizerFailed: "Reč sa nepodarilo rozpoznať. Skúste to znova.",
        openSettings: "Otvoriť nastavenia"
    )

    static let de = NotchVoiceReplyStrings(
        settingTitle: "Sprachantworten",
        settingHint: "Ein Mikrofon im Antwortfeld einer Sitzung: Sprich, und die Wörter erscheinen beim Sprechen. Die Sprache wird nur auf diesem Mac erkannt, nie auf einem Server.",
        language: "Sprache",
        followAppFormat: "Wie die App (%@)",
        notOnDeviceTag: "nicht auf diesem Mac",
        shortcut: "Zum Sprechen halten",
        shortcutHint: "Halte die Tasten und sprich mit der Sitzung auf dem Bildschirm oder mit der zuletzt aktiven. Loslassen beendet.",
        sendOnStop: "Senden, wenn ich aufhöre zu sprechen",
        sendOnStopHint: "Die Wörter gehen an die Sitzung, sobald die Aufnahme endet, ohne ⌘↩.",
        start: "Per Sprache antworten",
        stop: "Aufnahme beenden",
        listening: "Hört zu …",
        escCancels: "Esc bricht ab",
        noMicrophone: "Kein Mikrofon angeschlossen.",
        microphoneDenied: "Openbar darf das Mikrofon nicht verwenden.",
        speechDenied: "Openbar darf keine Sprache erkennen.",
        notOnDeviceFormat: "%@ kann auf diesem Mac nicht erkannt werden. Füge die Sprache unter Diktat in den Tastatur-Einstellungen hinzu.",
        dictationOff: "Schalte Diktat in den Tastatur-Einstellungen ein; ohne Diktat erkennt Apple auf diesem Mac keine Sprache.",
        recognizerFailed: "Die Sprache konnte nicht erkannt werden. Versuch es noch einmal.",
        openSettings: "Einstellungen öffnen"
    )

    static let fr = NotchVoiceReplyStrings(
        settingTitle: "Réponses vocales",
        settingHint: "Un micro dans le champ de réponse d’une session\u{00A0}: parlez, les mots s’écrivent au fur et à mesure. La parole est reconnue sur ce Mac uniquement, jamais sur un serveur.",
        language: "Langue",
        followAppFormat: "Comme l’app (%@)",
        notOnDeviceTag: "absente de ce Mac",
        shortcut: "Maintenir pour parler",
        shortcutHint: "Maintenez les touches et parlez à la session affichée, ou à la dernière au travail. Relâchez pour arrêter.",
        sendOnStop: "Envoyer quand je m’arrête de parler",
        sendOnStopHint: "Les mots partent vers la session dès la fin de l’enregistrement, sans ⌘↩.",
        start: "Répondre à voix haute",
        stop: "Arrêter l’enregistrement",
        listening: "À l’écoute…",
        escCancels: "Échap annule",
        noMicrophone: "Aucun micro n’est branché.",
        microphoneDenied: "Openbar n’est pas autorisé à utiliser le micro.",
        speechDenied: "Openbar n’est pas autorisé à reconnaître la parole.",
        notOnDeviceFormat: "%@ ne peut pas être reconnu sur ce Mac. Ajoutez la langue dans Dictée, dans les réglages Clavier.",
        dictationOff: "Activez la Dictée dans les réglages Clavier\u{00A0}; sans elle, Apple ne reconnaît pas la parole sur ce Mac.",
        recognizerFailed: "La parole n’a pas pu être reconnue. Réessayez.",
        openSettings: "Ouvrir les réglages"
    )

    static let it = NotchVoiceReplyStrings(
        settingTitle: "Risposte vocali",
        settingHint: "Un microfono nel campo di risposta della sessione: parla e le parole compaiono mentre parli. La voce è riconosciuta solo su questo Mac, mai su un server.",
        language: "Lingua",
        followAppFormat: "Come l’app (%@)",
        notOnDeviceTag: "non su questo Mac",
        shortcut: "Tieni premuto per parlare",
        shortcutHint: "Tieni premuti i tasti e parla alla sessione sullo schermo o all’ultima al lavoro. Rilascia per fermare.",
        sendOnStop: "Invia quando smetto di parlare",
        sendOnStopHint: "Le parole vanno alla sessione appena finisce la registrazione, senza ⌘↩.",
        start: "Rispondi a voce",
        stop: "Ferma registrazione",
        listening: "In ascolto…",
        escCancels: "Esc annulla",
        noMicrophone: "Nessun microfono collegato.",
        microphoneDenied: "Openbar non ha il permesso di usare il microfono.",
        speechDenied: "Openbar non ha il permesso di riconoscere la voce.",
        notOnDeviceFormat: "%@ non può essere riconosciuto su questo Mac. Aggiungilo in Dettatura nelle impostazioni Tastiera.",
        dictationOff: "Attiva Dettatura nelle impostazioni Tastiera; senza di essa Apple non riconosce la voce su questo Mac.",
        recognizerFailed: "Non è stato possibile riconoscere la voce. Riprova.",
        openSettings: "Apri Impostazioni"
    )

    static let ja = NotchVoiceReplyStrings(
        settingTitle: "音声で返信",
        settingHint: "セッションの返信欄にマイクを表示します。話すとその場で文字になります。音声認識はこのMac上だけで行い、サーバーには送りません。",
        language: "言語",
        followAppFormat: "アプリと同じ（%@）",
        notOnDeviceTag: "このMacでは使用不可",
        shortcut: "押している間話す",
        shortcutHint: "キーを押している間、表示中のセッション、または最後に動いていたセッションに話しかけます。離すと止まります。",
        sendOnStop: "話し終えたら送信",
        sendOnStopHint: "録音が終わるとすぐに、⌘↩なしでセッションに送ります。",
        start: "音声で返信",
        stop: "録音を停止",
        listening: "聞き取り中…",
        escCancels: "Escでキャンセル",
        noMicrophone: "マイクが接続されていません。",
        microphoneDenied: "Openbarにマイクの使用が許可されていません。",
        speechDenied: "Openbarに音声認識が許可されていません。",
        notOnDeviceFormat: "%@はこのMacで認識できません。キーボード設定の音声入力で追加してください。",
        dictationOff: "キーボード設定で音声入力をオンにしてください。オフのままでは、このMacで音声を認識できません。",
        recognizerFailed: "音声を認識できませんでした。もう一度お試しください。",
        openSettings: "設定を開く"
    )

    static let ko = NotchVoiceReplyStrings(
        settingTitle: "음성 답장",
        settingHint: "세션 답장 칸에 마이크가 생깁니다. 말하면 바로 글자로 채워집니다. 음성은 이 Mac에서만 인식되며 서버로 보내지 않습니다.",
        language: "언어",
        followAppFormat: "앱과 같게 (%@)",
        notOnDeviceTag: "이 Mac에서 사용 불가",
        shortcut: "누르고 말하기",
        shortcutHint: "키를 누른 채 화면의 세션이나 마지막으로 작업한 세션에 말하세요. 놓으면 멈춥니다.",
        sendOnStop: "말을 멈추면 보내기",
        sendOnStopHint: "녹음이 끝나는 즉시 ⌘↩ 없이 세션으로 보냅니다.",
        start: "음성으로 답장",
        stop: "녹음 중지",
        listening: "듣는 중…",
        escCancels: "Esc로 취소",
        noMicrophone: "연결된 마이크가 없습니다.",
        microphoneDenied: "Openbar에 마이크 사용 권한이 없습니다.",
        speechDenied: "Openbar에 음성 인식 권한이 없습니다.",
        notOnDeviceFormat: "이 Mac에서는 %@을(를) 인식할 수 없습니다. 키보드 설정의 받아쓰기에서 추가하세요.",
        dictationOff: "키보드 설정에서 받아쓰기를 켜세요. 꺼져 있으면 이 Mac에서 음성을 인식할 수 없습니다.",
        recognizerFailed: "음성을 인식하지 못했습니다. 다시 시도하세요.",
        openSettings: "설정 열기"
    )

    static let uk = NotchVoiceReplyStrings(
        settingTitle: "Голосові відповіді",
        settingHint: "Мікрофон у полі відповіді сесії: говоріть, і слова з’являються під час мовлення. Мова розпізнається лише на цьому Mac, ніколи на сервері.",
        language: "Мова",
        followAppFormat: "Як у застосунку (%@)",
        notOnDeviceTag: "немає на цьому Mac",
        shortcut: "Говорити, утримуючи",
        shortcutHint: "Утримуйте клавіші й говоріть до сесії на екрані або до тієї, що працювала останньою. Відпустіть, щоб зупинити.",
        sendOnStop: "Надсилати, коли я замовкаю",
        sendOnStopHint: "Слова йдуть у сесію, щойно запис закінчиться, без ⌘↩.",
        start: "Відповісти голосом",
        stop: "Зупинити запис",
        listening: "Слухаю…",
        escCancels: "Esc — скасувати",
        noMicrophone: "Мікрофон не під’єднано.",
        microphoneDenied: "Openbar не дозволено використовувати мікрофон.",
        speechDenied: "Openbar не дозволено розпізнавати мовлення.",
        notOnDeviceFormat: "%@ не можна розпізнати на цьому Mac. Додайте мову в розділі «Диктування» налаштувань клавіатури.",
        dictationOff: "Увімкніть диктування в налаштуваннях клавіатури: без нього Apple не розпізнає мовлення на цьому Mac.",
        recognizerFailed: "Не вдалося розпізнати мовлення. Спробуйте ще раз.",
        openSettings: "Відкрити налаштування"
    )

    static let zhHans = NotchVoiceReplyStrings(
        settingTitle: "语音回复",
        settingHint: "在会话的回复栏里放一个麦克风：开口说话，文字随说随出。语音只在这台 Mac 上识别，从不发送到服务器。",
        language: "语言",
        followAppFormat: "与应用相同（%@）",
        notOnDeviceTag: "此 Mac 不支持",
        shortcut: "按住说话",
        shortcutHint: "按住按键，对屏幕上的会话或最近在工作的会话说话。松开即停止。",
        sendOnStop: "说完后发送",
        sendOnStopHint: "录音一结束就发送到会话，无需 ⌘↩。",
        start: "语音回复",
        stop: "停止录音",
        listening: "正在聆听…",
        escCancels: "按 Esc 取消",
        noMicrophone: "没有连接麦克风。",
        microphoneDenied: "Openbar 没有使用麦克风的权限。",
        speechDenied: "Openbar 没有语音识别的权限。",
        notOnDeviceFormat: "此 Mac 无法识别%@。请在键盘设置的“听写”中添加该语言。",
        dictationOff: "请在键盘设置中打开“听写”；关闭时，这台 Mac 无法识别语音。",
        recognizerFailed: "无法识别语音，请重试。",
        openSettings: "打开设置"
    )

    static let zhTW = NotchVoiceReplyStrings(
        settingTitle: "語音回覆",
        settingHint: "在工作階段的回覆欄放一個麥克風：開口說話，文字隨說隨出。語音只在這台 Mac 上辨識，絕不傳到伺服器。",
        language: "語言",
        followAppFormat: "與 App 相同（%@）",
        notOnDeviceTag: "此 Mac 不支援",
        shortcut: "按住說話",
        shortcutHint: "按住按鍵，對畫面上的工作階段或最近在工作的工作階段說話。放開即停止。",
        sendOnStop: "說完後傳送",
        sendOnStopHint: "錄音一結束就傳送到工作階段，不必按 ⌘↩。",
        start: "語音回覆",
        stop: "停止錄音",
        listening: "正在聆聽…",
        escCancels: "按 Esc 取消",
        noMicrophone: "沒有連接麥克風。",
        microphoneDenied: "Openbar 沒有使用麥克風的權限。",
        speechDenied: "Openbar 沒有語音辨識的權限。",
        notOnDeviceFormat: "此 Mac 無法辨識%@。請在鍵盤設定的「聽寫」中加入該語言。",
        dictationOff: "請在鍵盤設定中打開「聽寫」；關閉時，這台 Mac 無法辨識語音。",
        recognizerFailed: "無法辨識語音，請再試一次。",
        openSettings: "打開設定"
    )

    static let zhHK = NotchVoiceReplyStrings(
        settingTitle: "語音回覆",
        settingHint: "喺工作階段嘅回覆欄放一個咪：開口講嘢，文字一路講一路出。語音只會喺呢部 Mac 上辨識，唔會傳去伺服器。",
        language: "語言",
        followAppFormat: "同 App 一樣（%@）",
        notOnDeviceTag: "呢部 Mac 不支援",
        shortcut: "按住講嘢",
        shortcutHint: "按住按鍵，對住畫面上嘅工作階段或者最近做緊嘢嘅工作階段講嘢。放手就停。",
        sendOnStop: "講完就傳送",
        sendOnStopHint: "錄音一完就傳去工作階段，唔使按 ⌘↩。",
        start: "語音回覆",
        stop: "停止錄音",
        listening: "聆聽中…",
        escCancels: "按 Esc 取消",
        noMicrophone: "未連接咪高峰。",
        microphoneDenied: "Openbar 未有使用咪高峰嘅權限。",
        speechDenied: "Openbar 未有語音辨識嘅權限。",
        notOnDeviceFormat: "呢部 Mac 辨識唔到%@。請喺鍵盤設定嘅「聽寫」加入呢種語言。",
        dictationOff: "請喺鍵盤設定打開「聽寫」；閂咗嘅話，呢部 Mac 辨識唔到語音。",
        recognizerFailed: "辨識唔到語音，請再試。",
        openSettings: "打開設定"
    )
}
