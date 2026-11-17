// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The update prompt (Mac UpdatePreviewView, Windows update window) and the
/// message shown when a download is refused for its signature.
struct UpdatePromptStrings {
    /// Prompt heading; %@ is the new version, such as 3.6.0.
    let titleFormat: String
    /// Under the heading; %@ is the running version.
    let currentFormat: String
    let whatsNew: String
    let noNotes: String
    let install: String
    let later: String
    let skip: String
    let relaunchNote: String
    let signatureRefusedTitle: String
    let signatureRefusedBody: String
    /// Follows “Update failed:” in Settings.
    let signatureRefusedShort: String
    let openReleasePage: String
    let close: String
}

extension FeatureStrings {
    static func updatePrompt(_ language: AppLanguage) -> UpdatePromptStrings {
        switch language {
        case .enUS:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ is available",
                currentFormat: "You have %@.",
                whatsNew: "What’s new",
                noNotes: "This release has no notes.",
                install: "Install and relaunch",
                later: "Remind me later",
                skip: "Skip this version",
                relaunchNote: "Openbar closes for a moment and comes back with your settings.",
                signatureRefusedTitle: "Update not installed",
                signatureRefusedBody: "The download isn’t signed by Openbar, or it changed on the way. It was deleted and nothing was installed.",
                signatureRefusedShort: "the signature doesn’t match",
                openReleasePage: "Open release page",
                close: "Close")
        case .ptBR:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ disponível",
                currentFormat: "Você tem a versão %@.",
                whatsNew: "Novidades",
                noNotes: "Esta versão não tem notas.",
                install: "Instalar e reabrir",
                later: "Lembrar depois",
                skip: "Pular esta versão",
                relaunchNote: "O Openbar fecha por um instante e volta com seus ajustes.",
                signatureRefusedTitle: "Atualização não instalada",
                signatureRefusedBody: "O download não tem a assinatura do Openbar ou foi alterado no caminho. Ele foi apagado e nada foi instalado.",
                signatureRefusedShort: "a assinatura não confere",
                openReleasePage: "Abrir página da versão",
                close: "Fechar")
        case .tr:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ hazır",
                currentFormat: "Sizdeki sürüm: %@.",
                whatsNew: "Yenilikler",
                noNotes: "Bu sürümün notu yok.",
                install: "Yükle ve yeniden aç",
                later: "Daha sonra hatırlat",
                skip: "Bu sürümü atla",
                relaunchNote: "Openbar kısa bir süre kapanır ve ayarlarınızla geri gelir.",
                signatureRefusedTitle: "Güncelleme yüklenmedi",
                signatureRefusedBody: "İndirilen dosya Openbar tarafından imzalanmamış ya da yolda değiştirilmiş. Silindi ve hiçbir şey yüklenmedi.",
                signatureRefusedShort: "imza eşleşmiyor",
                openReleasePage: "Sürüm sayfasını aç",
                close: "Kapat")
        case .ru:
            return UpdatePromptStrings(
                titleFormat: "Доступен Openbar %@",
                currentFormat: "У вас версия %@.",
                whatsNew: "Что нового",
                noNotes: "У этой версии нет заметок.",
                install: "Установить и перезапустить",
                later: "Напомнить позже",
                skip: "Пропустить эту версию",
                relaunchNote: "Openbar ненадолго закроется и вернётся с вашими настройками.",
                signatureRefusedTitle: "Обновление не установлено",
                signatureRefusedBody: "Загруженный файл не подписан Openbar или был изменён по пути. Он удалён, ничего не установлено.",
                signatureRefusedShort: "подпись не совпадает",
                openReleasePage: "Открыть страницу выпуска",
                close: "Закрыть")
        case .es:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ disponible",
                currentFormat: "Tienes la versión %@.",
                whatsNew: "Novedades",
                noNotes: "Esta versión no tiene notas.",
                install: "Instalar y reabrir",
                later: "Recordármelo más tarde",
                skip: "Omitir esta versión",
                relaunchNote: "Openbar se cierra un momento y vuelve con tus ajustes.",
                signatureRefusedTitle: "Actualización no instalada",
                signatureRefusedBody: "La descarga no está firmada por Openbar o cambió por el camino. Se eliminó y no se instaló nada.",
                signatureRefusedShort: "la firma no coincide",
                openReleasePage: "Abrir página de la versión",
                close: "Cerrar")
        case .sk:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ je k dispozícii",
                currentFormat: "Máte verziu %@.",
                whatsNew: "Čo je nové",
                noNotes: "Táto verzia nemá poznámky.",
                install: "Nainštalovať a znova otvoriť",
                later: "Pripomenúť neskôr",
                skip: "Preskočiť túto verziu",
                relaunchNote: "Openbar sa na chvíľu zavrie a vráti sa s vašimi nastaveniami.",
                signatureRefusedTitle: "Aktualizácia nebola nainštalovaná",
                signatureRefusedBody: "Stiahnutý súbor nie je podpísaný Openbarom alebo sa cestou zmenil. Bol vymazaný a nič sa nenainštalovalo.",
                signatureRefusedShort: "podpis nesúhlasí",
                openReleasePage: "Otvoriť stránku verzie",
                close: "Zavrieť")
        case .de:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ ist verfügbar",
                currentFormat: "Du hast Version %@.",
                whatsNew: "Neuerungen",
                noNotes: "Diese Version hat keine Notizen.",
                install: "Installieren und neu starten",
                later: "Später erinnern",
                skip: "Diese Version überspringen",
                relaunchNote: "Openbar schließt sich kurz und kommt mit deinen Einstellungen zurück.",
                signatureRefusedTitle: "Update nicht installiert",
                signatureRefusedBody: "Der Download ist nicht von Openbar signiert oder wurde unterwegs verändert. Er wurde gelöscht, nichts wurde installiert.",
                signatureRefusedShort: "die Signatur stimmt nicht",
                openReleasePage: "Release-Seite öffnen",
                close: "Schließen")
        case .fr:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ est disponible",
                currentFormat: "Vous avez la version %@.",
                whatsNew: "Nouveautés",
                noNotes: "Cette version n’a pas de notes.",
                install: "Installer et relancer",
                later: "Me le rappeler plus tard",
                skip: "Ignorer cette version",
                relaunchNote: "Openbar se ferme un instant et revient avec vos réglages.",
                signatureRefusedTitle: "Mise à jour non installée",
                signatureRefusedBody: "Le téléchargement n’est pas signé par Openbar ou a été modifié en route. Il a été supprimé et rien n’a été installé.",
                signatureRefusedShort: "la signature ne correspond pas",
                openReleasePage: "Ouvrir la page de la version",
                close: "Fermer")
        case .it:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ è disponibile",
                currentFormat: "Hai la versione %@.",
                whatsNew: "Novità",
                noNotes: "Questa versione non ha note.",
                install: "Installa e riapri",
                later: "Ricordamelo più tardi",
                skip: "Salta questa versione",
                relaunchNote: "Openbar si chiude per un attimo e torna con le tue impostazioni.",
                signatureRefusedTitle: "Aggiornamento non installato",
                signatureRefusedBody: "Il download non è firmato da Openbar o è stato modificato lungo la strada. È stato eliminato e non è stato installato nulla.",
                signatureRefusedShort: "la firma non corrisponde",
                openReleasePage: "Apri la pagina della versione",
                close: "Chiudi")
        case .ja:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ が利用できます",
                currentFormat: "現在のバージョンは %@ です。",
                whatsNew: "新機能",
                noNotes: "このリリースにはノートがありません。",
                install: "インストールして再起動",
                later: "後で通知",
                skip: "このバージョンをスキップ",
                relaunchNote: "Openbar は一時的に終了し、設定はそのままで戻ります。",
                signatureRefusedTitle: "アップデートはインストールされませんでした",
                signatureRefusedBody: "ダウンロードしたファイルは Openbar の署名がないか、途中で改ざんされています。削除され、何もインストールされていません。",
                signatureRefusedShort: "署名が一致しません",
                openReleasePage: "リリースページを開く",
                close: "閉じる")
        case .ko:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ 사용 가능",
                currentFormat: "현재 버전은 %@입니다.",
                whatsNew: "새로운 기능",
                noNotes: "이 릴리스에는 노트가 없습니다.",
                install: "설치 후 다시 열기",
                later: "나중에 알림",
                skip: "이 버전 건너뛰기",
                relaunchNote: "Openbar가 잠시 종료된 후 설정 그대로 다시 열립니다.",
                signatureRefusedTitle: "업데이트가 설치되지 않았습니다",
                signatureRefusedBody: "다운로드한 파일에 Openbar 서명이 없거나 전송 중 변경되었습니다. 파일을 삭제했으며 아무것도 설치하지 않았습니다.",
                signatureRefusedShort: "서명이 일치하지 않음",
                openReleasePage: "릴리스 페이지 열기",
                close: "닫기")
        case .uk:
            return UpdatePromptStrings(
                titleFormat: "Доступний Openbar %@",
                currentFormat: "У вас версія %@.",
                whatsNew: "Що нового",
                noNotes: "Цей випуск не має нотаток.",
                install: "Встановити й перезапустити",
                later: "Нагадати пізніше",
                skip: "Пропустити цю версію",
                relaunchNote: "Openbar ненадовго закриється й повернеться з вашими налаштуваннями.",
                signatureRefusedTitle: "Оновлення не встановлено",
                signatureRefusedBody: "Завантажений файл не підписаний Openbar або змінився дорогою. Його видалено, нічого не встановлено.",
                signatureRefusedShort: "підпис не збігається",
                openReleasePage: "Відкрити сторінку випуску",
                close: "Закрити")
        case .zhHans:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ 现已推出",
                currentFormat: "你当前的版本是 %@。",
                whatsNew: "新功能",
                noNotes: "此版本没有说明。",
                install: "安装并重新打开",
                later: "稍后提醒",
                skip: "跳过此版本",
                relaunchNote: "Openbar 会短暂退出，然后保留你的设置重新打开。",
                signatureRefusedTitle: "未安装更新",
                signatureRefusedBody: "下载的文件没有 Openbar 的签名，或在传输中被更改。它已被删除，没有安装任何内容。",
                signatureRefusedShort: "签名不匹配",
                openReleasePage: "打开发布页面",
                close: "关闭")
        case .zhTW:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ 已推出",
                currentFormat: "你目前的版本是 %@。",
                whatsNew: "新功能",
                noNotes: "此版本沒有說明。",
                install: "安裝並重新開啟",
                later: "稍後提醒",
                skip: "略過此版本",
                relaunchNote: "Openbar 會短暫結束，然後保留你的設定重新開啟。",
                signatureRefusedTitle: "未安裝更新",
                signatureRefusedBody: "下載的檔案沒有 Openbar 的簽署，或在傳輸中遭到更改。檔案已刪除，未安裝任何內容。",
                signatureRefusedShort: "簽署不符",
                openReleasePage: "打開發佈頁面",
                close: "關閉")
        case .zhHK:
            return UpdatePromptStrings(
                titleFormat: "Openbar %@ 已推出",
                currentFormat: "你目前的版本是 %@。",
                whatsNew: "新功能",
                noNotes: "此版本沒有說明。",
                install: "安裝並重新開啟",
                later: "稍後提醒",
                skip: "略過此版本",
                relaunchNote: "Openbar 會短暫結束，然後保留你的設定重新開啟。",
                signatureRefusedTitle: "未安裝更新",
                signatureRefusedBody: "下載的檔案沒有 Openbar 的簽署，或在傳輸中遭到更改。檔案已刪除，未安裝任何內容。",
                signatureRefusedShort: "簽署不符",
                openReleasePage: "打開發佈頁面",
                close: "關閉")
        }
    }
}
