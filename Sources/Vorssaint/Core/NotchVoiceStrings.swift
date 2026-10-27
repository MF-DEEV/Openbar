// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

struct NotchVoiceStrings {
    /// The activity's and its switch's name.
    let title: String
    let muted: String
    let unmuted: String
    let deafened: String
    let hint: String

    func status(_ reading: VoiceChatReading) -> String {
        reading.deafened ? deafened : reading.muted ? muted : unmuted
    }
}

extension FeatureStrings {
    static func notchVoice(_ language: AppLanguage) -> NotchVoiceStrings {
        switch language {
        case .enUS: return NotchVoiceStrings(
            title: "Microphone status",
            muted: "Microphone muted",
            unmuted: "Microphone unmuted",
            deafened: "Headphones and microphone muted",
            hint: "Muting and deafening in Discord show beside the camera, with a red mark while muted. Discord’s controls and shortcuts are only read, never changed.")
        case .ptBR: return NotchVoiceStrings(
            title: "Status do microfone",
            muted: "Microfone silenciado",
            unmuted: "Microfone ativado",
            deafened: "Fones e microfone silenciados",
            hint: "Silenciar e ensurdecer no Discord aparecem ao lado da câmera, com uma marca vermelha enquanto silenciado. Os controles e atalhos do Discord são só lidos, nunca alterados.")
        case .es: return NotchVoiceStrings(
            title: "Estado del micrófono",
            muted: "Micrófono silenciado",
            unmuted: "Micrófono activado",
            deafened: "Auriculares y micrófono silenciados",
            hint: "Silenciar y ensordecer en Discord aparecen junto a la cámara, con una marca roja mientras está silenciado. Los controles y atajos de Discord solo se leen, nunca se cambian.")
        case .sk: return NotchVoiceStrings(
            title: "Stav mikrofónu",
            muted: "Mikrofón je stlmený",
            unmuted: "Mikrofón je zapnutý",
            deafened: "Slúchadlá a mikrofón sú stlmené",
            hint: "Stlmenie mikrofónu a zvuku v Discorde sa zobrazí vedľa kamery a počas stlmenia ostane červená značka. Ovládacie prvky a skratky Discordu sa len čítajú, nikdy nemenia.")
        case .de: return NotchVoiceStrings(
            title: "Mikrofonstatus",
            muted: "Mikrofon stumm",
            unmuted: "Mikrofon an",
            deafened: "Kopfhörer und Mikrofon stumm",
            hint: "Stummschalten und Ton aus in Discord erscheinen neben der Kamera, mit roter Markierung, solange stummgeschaltet. Steuerelemente und Kurzbefehle von Discord werden nur gelesen, nie geändert.")
        case .fr: return NotchVoiceStrings(
            title: "État du micro",
            muted: "Micro coupé",
            unmuted: "Micro réactivé",
            deafened: "Casque et micro coupés",
            hint: "La coupure du micro et du son dans Discord s’affiche à côté de la caméra, avec une marque rouge tant que le micro est coupé. Les commandes et raccourcis de Discord sont seulement lus, jamais modifiés.")
        case .it: return NotchVoiceStrings(
            title: "Stato del microfono",
            muted: "Microfono silenziato",
            unmuted: "Microfono attivo",
            deafened: "Cuffie e microfono silenziati",
            hint: "Il silenziamento del microfono e dell’audio in Discord appare accanto alla fotocamera, con un segno rosso finché è silenziato. I controlli e le scorciatoie di Discord vengono solo letti, mai modificati.")
        case .ru: return NotchVoiceStrings(
            title: "Состояние микрофона",
            muted: "Микрофон выключен",
            unmuted: "Микрофон включён",
            deafened: "Наушники и микрофон выключены",
            hint: "Выключение микрофона и звука в Discord видно рядом с камерой, а пока микрофон выключен, остаётся красная отметка. Кнопки и сочетания клавиш Discord только читаются и никогда не меняются.")
        case .tr: return NotchVoiceStrings(
            title: "Mikrofon durumu",
            muted: "Mikrofon kapalı",
            unmuted: "Mikrofon açık",
            deafened: "Kulaklık ve mikrofon kapalı",
            hint: "Discord’da mikrofonu ve sesi kapatmak kameranın yanında görünür; mikrofon kapalıyken kırmızı bir işaret kalır. Discord’un denetimleri ve kısayolları yalnızca okunur, asla değiştirilmez.")
        case .ja: return NotchVoiceStrings(
            title: "マイクの状態",
            muted: "マイクはミュート中",
            unmuted: "マイクのミュートを解除",
            deafened: "ヘッドホンとマイクはミュート中",
            hint: "Discordのマイクミュートとスピーカーミュートがカメラの横に表示され、ミュート中は赤い印が残ります。Discordのコントロールとショートカットは読み取るだけで、変更しません。")
        case .ko: return NotchVoiceStrings(
            title: "마이크 상태",
            muted: "마이크 음소거됨",
            unmuted: "마이크 음소거 해제됨",
            deafened: "헤드폰과 마이크 음소거됨",
            hint: "Discord의 마이크 음소거와 헤드셋 음소거가 카메라 옆에 표시되고, 음소거 중에는 빨간 표시가 남습니다. Discord의 컨트롤과 단축키는 읽기만 하고 바꾸지 않습니다.")
        case .uk: return NotchVoiceStrings(
            title: "Стан мікрофона",
            muted: "Мікрофон вимкнено",
            unmuted: "Мікрофон увімкнено",
            deafened: "Навушники й мікрофон вимкнено",
            hint: "Вимкнення мікрофона й звуку в Discord видно поруч із камерою, а поки мікрофон вимкнено, лишається червона позначка. Кнопки й скорочення Discord лише читаються, ніколи не змінюються.")
        case .zhHans: return NotchVoiceStrings(
            title: "麦克风状态",
            muted: "麦克风已静音",
            unmuted: "麦克风已取消静音",
            deafened: "耳机和麦克风已静音",
            hint: "Discord 的麦克风静音和耳机静音会显示在摄像头旁，静音时保留红色标记。只读取 Discord 的控件和快捷键，从不更改。")
        case .zhTW: return NotchVoiceStrings(
            title: "麥克風狀態",
            muted: "麥克風已靜音",
            unmuted: "麥克風已取消靜音",
            deafened: "耳機和麥克風已靜音",
            hint: "Discord 的麥克風靜音和耳機靜音會顯示在相機旁，靜音時保留紅色標記。只讀取 Discord 的控制項和快速鍵，從不更改。")
        case .zhHK: return NotchVoiceStrings(
            title: "咪高峰狀態",
            muted: "咪高峰已靜音",
            unmuted: "咪高峰已取消靜音",
            deafened: "耳機和咪高峰已靜音",
            hint: "Discord 的咪高峰靜音和耳機靜音會顯示在相機旁，靜音時保留紅色標記。只讀取 Discord 的控制項和快速鍵，從不更改。")
        }
    }
}
