// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Reply to a Discord notification. The same text as the Windows and Linux
/// island (windows/src/i18n/overrides, windows.notifications.reply*).
struct DiscordReplyStrings {
    let reply: String
    let cancel: String
    let replyToFormat: String
    let placeholder: String
    let fieldLabel: String
    let send: String
    let sendLabel: String
    let hint: String
    let replyInDiscord: String
    let imageLabel: String
    let failures: [DiscordReplySupport.Failure: String]

    func failure(_ failure: DiscordReplySupport.Failure) -> String { failures[failure] ?? failures[.unavailable] ?? "" }

    /// "Reply to %@" with the sender filled in.
    func replyTo(_ name: String) -> String { replyToFormat.replacingOccurrences(of: "%@", with: name) }
}

extension FeatureStrings {
    static func discordReply(_ language: AppLanguage) -> DiscordReplyStrings {
        switch language {
        case .enUS: return DiscordReplyStrings(
            reply: "Reply",
            cancel: "Cancel",
            replyToFormat: "Reply to %@",
            placeholder: "Write a reply…",
            fieldLabel: "Reply to send in Discord",
            send: "Send",
            sendLabel: "Send the reply in Discord",
            hint: "Enter sends · Shift+Enter adds a line",
            replyInDiscord: "Reply in Discord",
            imageLabel: "Image from the Discord message",
            failures: [
                .noChat: "Can’t tell which chat this came from. Your draft is kept.",
                .tooLong: "Discord takes up to 2000 characters.",
                .openFailed: "Couldn’t open Discord. Your draft is kept.",
                .inspectFailed: "Couldn’t find Discord’s message box. Your draft is kept.",
                .wrongChat: "Open this chat in Discord, then try again. Your draft is kept.",
                .draftInDiscord: "This chat in Discord already has unsent text. Send or clear it there first.",
                .focusLost: "Discord lost focus. Your draft is kept.",
                .typingInterrupted: "Discord lost focus while typing. Check its message box; nothing was sent.",
                .inputRefused: "Discord didn’t take the text. Check its message box; nothing was sent.",
                .gone: "This notification is gone.",
                .unavailable: "Couldn’t send the reply. Your draft is kept."
            ])
        case .ptBR: return DiscordReplyStrings(
            reply: "Responder",
            cancel: "Cancelar",
            replyToFormat: "Responder a %@",
            placeholder: "Escreva uma resposta…",
            fieldLabel: "Resposta para enviar no Discord",
            send: "Enviar",
            sendLabel: "Enviar a resposta no Discord",
            hint: "Enter envia · Shift+Enter adiciona uma linha",
            replyInDiscord: "Responder no Discord",
            imageLabel: "Imagem da mensagem do Discord",
            failures: [
                .noChat: "Não dá para saber de qual chat isso veio. Seu rascunho foi mantido.",
                .tooLong: "O Discord aceita até 2000 caracteres.",
                .openFailed: "Não foi possível abrir o Discord. Seu rascunho foi mantido.",
                .inspectFailed: "A caixa de mensagem do Discord não foi encontrada. Seu rascunho foi mantido.",
                .wrongChat: "Abra este chat no Discord e tente de novo. Seu rascunho foi mantido.",
                .draftInDiscord: "Este chat do Discord já tem texto não enviado. Envie ou apague lá primeiro.",
                .focusLost: "O Discord perdeu o foco. Seu rascunho foi mantido.",
                .typingInterrupted: "O Discord perdeu o foco durante a digitação. Confira a caixa de mensagem; nada foi enviado.",
                .inputRefused: "O Discord não aceitou o texto. Confira a caixa de mensagem; nada foi enviado.",
                .gone: "Esta notificação não existe mais.",
                .unavailable: "Não foi possível enviar a resposta. Seu rascunho foi mantido."
            ])
        case .es: return DiscordReplyStrings(
            reply: "Responder",
            cancel: "Cancelar",
            replyToFormat: "Responder a %@",
            placeholder: "Escribe una respuesta…",
            fieldLabel: "Respuesta para enviar en Discord",
            send: "Enviar",
            sendLabel: "Enviar la respuesta en Discord",
            hint: "Intro envía · Mayús+Intro añade una línea",
            replyInDiscord: "Responder en Discord",
            imageLabel: "Imagen del mensaje de Discord",
            failures: [
                .noChat: "No se sabe de qué chat viene. Tu borrador se conserva.",
                .tooLong: "Discord admite hasta 2000 caracteres.",
                .openFailed: "No se pudo abrir Discord. Tu borrador se conserva.",
                .inspectFailed: "No se encontró el cuadro de mensaje de Discord. Tu borrador se conserva.",
                .wrongChat: "Abre este chat en Discord y vuelve a intentarlo. Tu borrador se conserva.",
                .draftInDiscord: "Este chat de Discord ya tiene texto sin enviar. Envíalo o bórralo allí primero.",
                .focusLost: "Discord perdió el foco. Tu borrador se conserva.",
                .typingInterrupted: "Discord perdió el foco mientras escribía. Revisa su cuadro de mensaje; no se envió nada.",
                .inputRefused: "Discord no aceptó el texto. Revisa su cuadro de mensaje; no se envió nada.",
                .gone: "Esta notificación ya no está.",
                .unavailable: "No se pudo enviar la respuesta. Tu borrador se conserva."
            ])
        case .sk: return DiscordReplyStrings(
            reply: "Odpovedať",
            cancel: "Zrušiť",
            replyToFormat: "Odpoveď pre %@",
            placeholder: "Napíšte odpoveď…",
            fieldLabel: "Odpoveď na odoslanie v Discorde",
            send: "Odoslať",
            sendLabel: "Odoslať odpoveď v Discorde",
            hint: "Enter odošle · Shift+Enter pridá riadok",
            replyInDiscord: "Odpovedať v Discorde",
            imageLabel: "Obrázok zo správy v Discorde",
            failures: [
                .noChat: "Nedá sa zistiť, z ktorého chatu to prišlo. Koncept zostáva.",
                .tooLong: "Discord prijme najviac 2000 znakov.",
                .openFailed: "Discord sa nepodarilo otvoriť. Koncept zostáva.",
                .inspectFailed: "Pole na správu v Discorde sa nenašlo. Koncept zostáva.",
                .wrongChat: "Otvorte tento chat v Discorde a skúste to znova. Koncept zostáva.",
                .draftInDiscord: "V tomto chate v Discorde už je neodoslaný text. Najprv ho tam odošlite alebo vymažte.",
                .focusLost: "Discord stratil fokus. Koncept zostáva.",
                .typingInterrupted: "Discord stratil fokus počas písania. Skontrolujte pole na správu; nič sa neodoslalo.",
                .inputRefused: "Discord text neprijal. Skontrolujte pole na správu; nič sa neodoslalo.",
                .gone: "Toto upozornenie už nie je k dispozícii.",
                .unavailable: "Odpoveď sa nepodarilo odoslať. Koncept zostáva."
            ])
        case .de: return DiscordReplyStrings(
            reply: "Antworten",
            cancel: "Abbrechen",
            replyToFormat: "Antwort an %@",
            placeholder: "Antwort schreiben…",
            fieldLabel: "Antwort, die in Discord gesendet wird",
            send: "Senden",
            sendLabel: "Antwort in Discord senden",
            hint: "Enter sendet · Umschalt+Enter fügt eine Zeile hinzu",
            replyInDiscord: "In Discord antworten",
            imageLabel: "Bild aus der Discord-Nachricht",
            failures: [
                .noChat: "Unklar, aus welchem Chat das kam. Dein Entwurf bleibt erhalten.",
                .tooLong: "Discord erlaubt bis zu 2000 Zeichen.",
                .openFailed: "Discord ließ sich nicht öffnen. Dein Entwurf bleibt erhalten.",
                .inspectFailed: "Das Nachrichtenfeld von Discord wurde nicht gefunden. Dein Entwurf bleibt erhalten.",
                .wrongChat: "Öffne diesen Chat in Discord und versuch es noch einmal. Dein Entwurf bleibt erhalten.",
                .draftInDiscord: "In diesem Discord-Chat steht schon ungesendeter Text. Sende oder lösche ihn dort zuerst.",
                .focusLost: "Discord hat den Fokus verloren. Dein Entwurf bleibt erhalten.",
                .typingInterrupted: "Discord hat beim Tippen den Fokus verloren. Prüfe das Nachrichtenfeld; nichts wurde gesendet.",
                .inputRefused: "Discord hat den Text nicht angenommen. Prüfe das Nachrichtenfeld; nichts wurde gesendet.",
                .gone: "Diese Benachrichtigung ist nicht mehr da.",
                .unavailable: "Die Antwort konnte nicht gesendet werden. Dein Entwurf bleibt erhalten."
            ])
        case .fr: return DiscordReplyStrings(
            reply: "Répondre",
            cancel: "Annuler",
            replyToFormat: "Répondre à %@",
            placeholder: "Écrire une réponse…",
            fieldLabel: "Réponse à envoyer dans Discord",
            send: "Envoyer",
            sendLabel: "Envoyer la réponse dans Discord",
            hint: "Entrée envoie · Maj+Entrée ajoute une ligne",
            replyInDiscord: "Répondre dans Discord",
            imageLabel: "Image du message Discord",
            failures: [
                .noChat: "Impossible de savoir de quel salon cela vient. Ton brouillon est conservé.",
                .tooLong: "Discord accepte jusqu’à 2000 caractères.",
                .openFailed: "Impossible d’ouvrir Discord. Ton brouillon est conservé.",
                .inspectFailed: "Champ de message de Discord introuvable. Ton brouillon est conservé.",
                .wrongChat: "Ouvre cette discussion dans Discord, puis réessaie. Ton brouillon est conservé.",
                .draftInDiscord: "Cette discussion Discord contient déjà du texte non envoyé. Envoie-le ou efface-le d’abord.",
                .focusLost: "Discord a perdu le focus. Ton brouillon est conservé.",
                .typingInterrupted: "Discord a perdu le focus pendant la saisie. Vérifie son champ de message ; rien n’a été envoyé.",
                .inputRefused: "Discord n’a pas accepté le texte. Vérifie son champ de message ; rien n’a été envoyé.",
                .gone: "Cette notification n’existe plus.",
                .unavailable: "Impossible d’envoyer la réponse. Ton brouillon est conservé."
            ])
        case .it: return DiscordReplyStrings(
            reply: "Rispondi",
            cancel: "Annulla",
            replyToFormat: "Rispondi a %@",
            placeholder: "Scrivi una risposta…",
            fieldLabel: "Risposta da inviare su Discord",
            send: "Invia",
            sendLabel: "Invia la risposta su Discord",
            hint: "Invio invia · Maiusc+Invio aggiunge una riga",
            replyInDiscord: "Rispondi su Discord",
            imageLabel: "Immagine dal messaggio di Discord",
            failures: [
                .noChat: "Non è chiaro da quale chat arrivi. La bozza resta salvata.",
                .tooLong: "Discord accetta fino a 2000 caratteri.",
                .openFailed: "Impossibile aprire Discord. La bozza resta salvata.",
                .inspectFailed: "Casella dei messaggi di Discord non trovata. La bozza resta salvata.",
                .wrongChat: "Apri questa chat su Discord e riprova. La bozza resta salvata.",
                .draftInDiscord: "In questa chat di Discord c’è già testo non inviato. Invialo o cancellalo prima lì.",
                .focusLost: "Discord ha perso il focus. La bozza resta salvata.",
                .typingInterrupted: "Discord ha perso il focus durante la digitazione. Controlla la casella dei messaggi; non è stato inviato nulla.",
                .inputRefused: "Discord non ha accettato il testo. Controlla la casella dei messaggi; non è stato inviato nulla.",
                .gone: "Questa notifica non c’è più.",
                .unavailable: "Impossibile inviare la risposta. La bozza resta salvata."
            ])
        case .ru: return DiscordReplyStrings(
            reply: "Ответить",
            cancel: "Отмена",
            replyToFormat: "Ответ для %@",
            placeholder: "Напишите ответ…",
            fieldLabel: "Ответ для отправки в Discord",
            send: "Отправить",
            sendLabel: "Отправить ответ в Discord",
            hint: "Enter отправляет · Shift+Enter добавляет строку",
            replyInDiscord: "Ответить в Discord",
            imageLabel: "Изображение из сообщения Discord",
            failures: [
                .noChat: "Не удалось понять, из какого чата это уведомление. Черновик сохранён.",
                .tooLong: "Discord принимает до 2000 символов.",
                .openFailed: "Не удалось открыть Discord. Черновик сохранён.",
                .inspectFailed: "Не найдено поле сообщения Discord. Черновик сохранён.",
                .wrongChat: "Откройте этот чат в Discord и попробуйте снова. Черновик сохранён.",
                .draftInDiscord: "В этом чате Discord уже есть неотправленный текст. Сначала отправьте или удалите его там.",
                .focusLost: "Discord потерял фокус. Черновик сохранён.",
                .typingInterrupted: "Discord потерял фокус во время ввода. Проверьте поле сообщения; ничего не отправлено.",
                .inputRefused: "Discord не принял текст. Проверьте поле сообщения; ничего не отправлено.",
                .gone: "Этого уведомления больше нет.",
                .unavailable: "Не удалось отправить ответ. Черновик сохранён."
            ])
        case .tr: return DiscordReplyStrings(
            reply: "Yanıtla",
            cancel: "İptal",
            replyToFormat: "%@ kişisine yanıt",
            placeholder: "Bir yanıt yazın…",
            fieldLabel: "Discord’da gönderilecek yanıt",
            send: "Gönder",
            sendLabel: "Yanıtı Discord’da gönder",
            hint: "Enter gönderir · Shift+Enter satır ekler",
            replyInDiscord: "Discord’da yanıtla",
            imageLabel: "Discord mesajındaki görsel",
            failures: [
                .noChat: "Bunun hangi sohbetten geldiği anlaşılamadı. Taslağınız duruyor.",
                .tooLong: "Discord en fazla 2000 karakter kabul eder.",
                .openFailed: "Discord açılamadı. Taslağınız duruyor.",
                .inspectFailed: "Discord’un mesaj kutusu bulunamadı. Taslağınız duruyor.",
                .wrongChat: "Bu sohbeti Discord’da açıp tekrar deneyin. Taslağınız duruyor.",
                .draftInDiscord: "Discord’daki bu sohbette gönderilmemiş metin var. Önce orada gönderin ya da silin.",
                .focusLost: "Discord odağı kaybetti. Taslağınız duruyor.",
                .typingInterrupted: "Discord yazarken odağı kaybetti. Mesaj kutusunu kontrol edin; hiçbir şey gönderilmedi.",
                .inputRefused: "Discord metni almadı. Mesaj kutusunu kontrol edin; hiçbir şey gönderilmedi.",
                .gone: "Bu bildirim artık yok.",
                .unavailable: "Yanıt gönderilemedi. Taslağınız duruyor."
            ])
        case .ja: return DiscordReplyStrings(
            reply: "返信",
            cancel: "キャンセル",
            replyToFormat: "%@ に返信",
            placeholder: "返信を入力…",
            fieldLabel: "Discord に送る返信",
            send: "送信",
            sendLabel: "Discord で返信を送信",
            hint: "Enter で送信 · Shift+Enter で改行",
            replyInDiscord: "Discord で返信",
            imageLabel: "Discord メッセージの画像",
            failures: [
                .noChat: "どのチャットからの通知か判別できません。下書きは残っています。",
                .tooLong: "Discord は 2000 文字までです。",
                .openFailed: "Discord を開けませんでした。下書きは残っています。",
                .inspectFailed: "Discord のメッセージ欄が見つかりません。下書きは残っています。",
                .wrongChat: "Discord でこのチャットを開いてから、もう一度試してください。下書きは残っています。",
                .draftInDiscord: "Discord のこのチャットに未送信のテキストがあります。先にそちらで送信するか消してください。",
                .focusLost: "Discord のフォーカスが外れました。下書きは残っています。",
                .typingInterrupted: "入力中に Discord のフォーカスが外れました。メッセージ欄を確認してください。何も送信されていません。",
                .inputRefused: "Discord がテキストを受け付けませんでした。メッセージ欄を確認してください。何も送信されていません。",
                .gone: "この通知はもうありません。",
                .unavailable: "返信を送信できませんでした。下書きは残っています。"
            ])
        case .ko: return DiscordReplyStrings(
            reply: "답장",
            cancel: "취소",
            replyToFormat: "%@에게 답장",
            placeholder: "답장 입력…",
            fieldLabel: "Discord로 보낼 답장",
            send: "보내기",
            sendLabel: "Discord에서 답장 보내기",
            hint: "Enter로 보내기 · Shift+Enter로 줄 바꿈",
            replyInDiscord: "Discord에서 답장",
            imageLabel: "Discord 메시지의 이미지",
            failures: [
                .noChat: "어느 채팅에서 온 알림인지 알 수 없습니다. 초안은 그대로 있습니다.",
                .tooLong: "Discord는 최대 2000자까지 보낼 수 있습니다.",
                .openFailed: "Discord를 열 수 없습니다. 초안은 그대로 있습니다.",
                .inspectFailed: "Discord 메시지 입력란을 찾을 수 없습니다. 초안은 그대로 있습니다.",
                .wrongChat: "Discord에서 이 채팅을 연 뒤 다시 시도하세요. 초안은 그대로 있습니다.",
                .draftInDiscord: "Discord의 이 채팅에 보내지 않은 텍스트가 있습니다. 먼저 거기서 보내거나 지우세요.",
                .focusLost: "Discord의 포커스가 사라졌습니다. 초안은 그대로 있습니다.",
                .typingInterrupted: "입력 중에 Discord의 포커스가 사라졌습니다. 메시지 입력란을 확인하세요. 아무것도 보내지 않았습니다.",
                .inputRefused: "Discord가 텍스트를 받지 않았습니다. 메시지 입력란을 확인하세요. 아무것도 보내지 않았습니다.",
                .gone: "이 알림은 더 이상 없습니다.",
                .unavailable: "답장을 보낼 수 없습니다. 초안은 그대로 있습니다."
            ])
        case .zhHans: return DiscordReplyStrings(
            reply: "回复",
            cancel: "取消",
            replyToFormat: "回复 %@",
            placeholder: "输入回复…",
            fieldLabel: "要在 Discord 中发送的回复",
            send: "发送",
            sendLabel: "在 Discord 中发送回复",
            hint: "Enter 发送 · Shift+Enter 换行",
            replyInDiscord: "在 Discord 中回复",
            imageLabel: "Discord 消息中的图片",
            failures: [
                .noChat: "无法判断这来自哪个聊天。草稿已保留。",
                .tooLong: "Discord 最多接受 2000 个字符。",
                .openFailed: "无法打开 Discord。草稿已保留。",
                .inspectFailed: "找不到 Discord 的消息框。草稿已保留。",
                .wrongChat: "请先在 Discord 中打开这个聊天，然后重试。草稿已保留。",
                .draftInDiscord: "Discord 的这个聊天里已有未发送的文字。请先在那里发送或清除。",
                .focusLost: "Discord 失去了焦点。草稿已保留。",
                .typingInterrupted: "输入时 Discord 失去了焦点。请检查它的消息框；没有发送任何内容。",
                .inputRefused: "Discord 没有接收文字。请检查它的消息框；没有发送任何内容。",
                .gone: "这条通知已不存在。",
                .unavailable: "无法发送回复。草稿已保留。"
            ])
        case .zhTW: return DiscordReplyStrings(
            reply: "回覆",
            cancel: "取消",
            replyToFormat: "回覆 %@",
            placeholder: "輸入回覆…",
            fieldLabel: "要在 Discord 傳送的回覆",
            send: "傳送",
            sendLabel: "在 Discord 傳送回覆",
            hint: "Enter 傳送 · Shift+Enter 換行",
            replyInDiscord: "在 Discord 回覆",
            imageLabel: "Discord 訊息中的圖片",
            failures: [
                .noChat: "無法判斷這來自哪個聊天。草稿已保留。",
                .tooLong: "Discord 最多接受 2000 個字元。",
                .openFailed: "無法開啟 Discord。草稿已保留。",
                .inspectFailed: "找不到 Discord 的訊息框。草稿已保留。",
                .wrongChat: "請先在 Discord 開啟這個聊天，再試一次。草稿已保留。",
                .draftInDiscord: "Discord 的這個聊天已有未傳送的文字。請先在那裡傳送或清除。",
                .focusLost: "Discord 失去了焦點。草稿已保留。",
                .typingInterrupted: "輸入時 Discord 失去了焦點。請檢查它的訊息框；沒有傳送任何內容。",
                .inputRefused: "Discord 沒有接收文字。請檢查它的訊息框；沒有傳送任何內容。",
                .gone: "這則通知已不存在。",
                .unavailable: "無法傳送回覆。草稿已保留。"
            ])
        case .zhHK: return DiscordReplyStrings(
            reply: "回覆",
            cancel: "取消",
            replyToFormat: "回覆 %@",
            placeholder: "輸入回覆…",
            fieldLabel: "要在 Discord 傳送的回覆",
            send: "傳送",
            sendLabel: "在 Discord 傳送回覆",
            hint: "Enter 傳送 · Shift+Enter 換行",
            replyInDiscord: "在 Discord 回覆",
            imageLabel: "Discord 訊息中的圖片",
            failures: [
                .noChat: "無法判斷這來自哪個聊天。草稿已保留。",
                .tooLong: "Discord 最多接受 2000 個字元。",
                .openFailed: "無法開啟 Discord。草稿已保留。",
                .inspectFailed: "找不到 Discord 的訊息框。草稿已保留。",
                .wrongChat: "請先在 Discord 開啟這個聊天，再試一次。草稿已保留。",
                .draftInDiscord: "Discord 的這個聊天已有未傳送的文字。請先在那裡傳送或清除。",
                .focusLost: "Discord 失去了焦點。草稿已保留。",
                .typingInterrupted: "輸入時 Discord 失去了焦點。請檢查它的訊息框；沒有傳送任何內容。",
                .inputRefused: "Discord 沒有接收文字。請檢查它的訊息框；沒有傳送任何內容。",
                .gone: "這則通知已不存在。",
                .unavailable: "無法傳送回覆。草稿已保留。"
            ])
        case .uk: return DiscordReplyStrings(
            reply: "Відповісти",
            cancel: "Скасувати",
            replyToFormat: "Відповідь для %@",
            placeholder: "Напишіть відповідь…",
            fieldLabel: "Відповідь для надсилання в Discord",
            send: "Надіслати",
            sendLabel: "Надіслати відповідь у Discord",
            hint: "Enter надсилає · Shift+Enter додає рядок",
            replyInDiscord: "Відповісти в Discord",
            imageLabel: "Зображення з повідомлення Discord",
            failures: [
                .noChat: "Не вдалося зрозуміти, з якого чату це сповіщення. Чернетку збережено.",
                .tooLong: "Discord приймає до 2000 символів.",
                .openFailed: "Не вдалося відкрити Discord. Чернетку збережено.",
                .inspectFailed: "Не знайдено поле повідомлення Discord. Чернетку збережено.",
                .wrongChat: "Відкрийте цей чат у Discord і спробуйте ще раз. Чернетку збережено.",
                .draftInDiscord: "У цьому чаті Discord уже є ненадісланий текст. Спершу надішліть або видаліть його там.",
                .focusLost: "Discord втратив фокус. Чернетку збережено.",
                .typingInterrupted: "Discord втратив фокус під час введення. Перевірте поле повідомлення; нічого не надіслано.",
                .inputRefused: "Discord не прийняв текст. Перевірте поле повідомлення; нічого не надіслано.",
                .gone: "Цього сповіщення більше немає.",
                .unavailable: "Не вдалося надіслати відповідь. Чернетку збережено."
            ])
        }
    }
}
