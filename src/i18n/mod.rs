//! UI localization only. Stable CLI names and JSON fields are never translated.
mod console;
mod messages;
mod surface;
use crate::control::store::{self, Store};
use anyhow::{ensure, Result};
pub use console::TRANSLATIONS as CONSOLE_TRANSLATIONS;
pub use messages::TRANSLATIONS as MESSAGE_TRANSLATIONS;
pub use messages::{change, diagnostic, diagnostic_for, format, format_for, reason, Notice};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
pub use surface::TRANSLATIONS as SURFACE_TRANSLATIONS;
pub use surface::{label, preset_key, preset_name};
pub const LANGUAGES: [&str; 6] = ["en", "zh-CN", "zh-TW", "ja", "de", "es"];
static CURRENT: AtomicUsize = AtomicUsize::new(0);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    language: String,
}
pub fn normalize(code: &str) -> usize {
    let code = code.replace('_', "-").to_ascii_lowercase();
    if code.starts_with("zh-tw") || code.starts_with("zh-hk") || code.starts_with("zh-hant") {
        2
    } else if code.starts_with("zh") {
        1
    } else if code.starts_with("ja") {
        3
    } else if code.starts_with("de") {
        4
    } else if code.starts_with("es") {
        5
    } else {
        0
    }
}
/// Native language names remain recognizable before a user can read the current locale.
pub fn language_name(code: &str) -> &'static str {
    [
        "English",
        "简体中文",
        "繁體中文",
        "日本語",
        "Deutsch",
        "Español",
    ][normalize(code)]
}
pub fn code() -> &'static str {
    LANGUAGES[CURRENT.load(Ordering::Relaxed).min(5)]
}
pub fn configure(store: &Store, explicit: Option<&str>) -> Result<()> {
    let saved = store::read_json::<Preferences>(&store.directory.join("ui.json")).ok();
    let env = ["MARIS_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty());
    let requested = explicit
        .or_else(|| saved.as_ref().map(|v| v.language.as_str()))
        .or(env.as_deref())
        .unwrap_or("en");
    CURRENT.store(normalize(requested), Ordering::Relaxed);
    Ok(())
}
pub fn save(store: &Store, language: &str) -> Result<()> {
    ensure!(
        LANGUAGES.contains(&language),
        "Supported languages: en, zh-CN, zh-TW, ja, de, es"
    );
    store.write_json(
        "ui.json",
        &Preferences {
            language: language.to_owned(),
        },
    )?;
    CURRENT.store(normalize(language), Ordering::Relaxed);
    Ok(())
}
pub fn cycle(store: &Store) -> Result<()> {
    let next = (CURRENT.load(Ordering::Relaxed) + 1) % LANGUAGES.len();
    save(store, LANGUAGES[next])
}
pub fn text(key: &str) -> &str {
    for_language(code(), key)
}
pub fn entries() -> impl Iterator<Item = &'static (&'static str, [&'static str; 5])> {
    TRANSLATIONS
        .iter()
        .chain(CONSOLE_TRANSLATIONS)
        .chain(SURFACE_TRANSLATIONS)
        .chain(MESSAGE_TRANSLATIONS)
}
pub fn has_translation(key: &str) -> bool {
    entries().any(|(source, _)| *source == key.trim())
}

/// Poll only on a UI/control thread. Explicit startup language survives until a
/// saved preference actually changes, then all attached windows follow it.
pub struct Watcher {
    saved: Option<String>,
}
impl Watcher {
    fn saved(store: &Store) -> Option<String> {
        store::read_json::<Preferences>(&store.directory.join("ui.json"))
            .ok()
            .map(|value| value.language)
            .filter(|language| LANGUAGES.contains(&language.as_str()))
    }
    pub fn new(store: &Store) -> Self {
        Self {
            saved: Self::saved(store),
        }
    }
    pub fn refresh(&mut self, store: &Store) -> bool {
        let saved = Self::saved(store);
        if saved == self.saved {
            return false;
        }
        self.saved = saved;
        if let Some(language) = &self.saved {
            CURRENT.store(normalize(language), Ordering::Relaxed);
            return true;
        }
        false
    }
}
pub const REQUIRED_CONTROL_KEYS: &[&str] = &[
    "Dashboard",
    "EQ Detail",
    "Mixer Matrix",
    "Device Detail",
    "Intelligence Detail",
    "Diagnostics",
    "Now Playing",
    "Sound",
    "Mixer",
    "Devices",
    "Intelligence",
    "System",
    "Real-time spectrum",
    "Waiting for signal",
    "EQ response",
    "AutoEq",
    "Capability",
    "Correction",
    "Dynamic EQ",
    "A/B",
    "Genre",
    "Instruments",
    "Vocal",
    "Theme source",
    "Confidence",
    "Model",
    "Music context",
    "Current plan",
    "Delta",
    "Preview ready",
    "Waiting for evidence",
    "AI tuning",
    "Strips",
    "Hardware I/O",
    "Mixer summary",
    "Revision",
    "System health",
    "Theme",
    "Signal",
    "Semantic",
    "Selected output",
    "Active output",
    "Output mode",
    "Profile key",
    "Stable ID",
    "Binding source",
    "Reported latency",
    "Rebinds",
    "Audio applications",
    "Scope",
    "Running",
    "Application",
    "Application audio metadata is unavailable on this platform.",
    "Confirm audio",
    "Y confirms · any other key cancels",
    "Changes",
    "Y apply · Esc discard · U undo after apply",
    "AI preview",
    "Shortcuts",
    "Dashboard: ↑/↓ select Sound · ←/→ adjust current device",
    "EQ Detail: ↑/↓ band · ←/→ gain · [/] Q · -/+ preamp",
    "Mixer: ↑/↓ application · Space toggle · Shift+A only · A all system audio",
    "O select output · Shift+O follow system default",
    "P presets · B A/B · K save device · G/J/Y AI",
    "Tab details · Esc dashboard · M compact · L language · S stop · Q close",
    "↑↓ Sound · ←→ Adjust",
    "↑↓ Band · ←→ Gain · [/] Q",
    "↑↓ App · Space Toggle · Shift+A Only · A All",
    "Tab Next · Esc Dashboard",
    "Help",
    "Downloaded",
    "Integrated",
    "Researched",
    "Yes",
    "No",
    "Input",
    "Output",
    "Sample rate",
    "Engine",
    "Current preset",
    "State",
    "Signal path",
    "Live status",
    "Unavailable",
    "Available",
    "Underruns",
    "Overruns",
    "Adaptive reduction",
    "Source",
    "Meter",
    "Mixer strips",
    "Matrix engine",
    "Hardware multi-input",
    "Dual bus engine",
    "Hardware multi-output",
    "Ducking",
    "Per-strip DSP",
    "Mixer capability",
    "Direction",
    "Device",
    "Default",
    "Language",
    "Backend",
    "Clock bridge",
    "Preset catalog",
    "Category",
    "Name",
    "Stable ID",
    "Safe preamp",
    "Bandwidth",
    "No preset available",
    "Preset details",
    "Live spectrum",
    "Playback",
    "Output level",
    "Headroom",
    "Loudness",
    "Health",
    "Device profile",
    "AutoEq model",
    "Match confidence",
    "Physical limits",
    "Virtual bass",
    "Unknown",
    "Unknown limits are never guessed from the music signal.",
    "Bass Assist",
    "Inputs",
    "Outputs",
    "Signal analysis",
    "Models",
    "RMS",
    "Crest",
    "Momentary LUFS",
    "Short-term LUFS",
    "True peak",
    "Stereo correlation",
    "Ready",
    "Candidate",
    "Research",
    "Models never run on the real-time audio callback.",
    "INPUT SPECTRUM · PRE-EQ",
    "No live signal",
    "Agents read capabilities and revisions before proposing changes.",
    "Preview, apply and undo use the same validation path as manual controls.",
    "No agent tool can execute arbitrary shell commands through Maris.",
    "Per-strip meters appear only after a real multi-input backend is connected.",
    "L cycles the UI language immediately.",
    "Device names and technical identifiers are never translated.",
];
pub fn for_language<'a>(language: &str, key: &'a str) -> &'a str {
    let index = normalize(language);
    if index == 0 {
        return surface::english(key);
    }
    entries()
        .find(|(source, _)| *source == key.trim())
        .map_or_else(|| surface::english(key), |(_, values)| values[index - 1])
}
// Each source key has all five translations; unknown diagnostics fall back to English.
pub const TRANSLATIONS: &[(&str, [&str; 5])] = &[
    ("Dashboard", ["总控台", "總控台", "ダッシュボード", "Dashboard", "Panel principal"]),
    ("EQ Detail", ["EQ 详情", "EQ 詳情", "EQ 詳細", "EQ-Details", "Detalle de EQ"]),
    ("Mixer Matrix", ["应用混音", "應用混音", "アプリミキサー", "App-Mixer", "Mezclador de apps"]),
    ("Device Detail", ["设备详情", "裝置詳情", "デバイス詳細", "Gerätedetails", "Detalle del dispositivo"]),
    ("Intelligence Detail", ["调音建议", "調音建議", "音質調整の提案", "Klangvorschläge", "Sugerencias de ajuste"]),
    ("Diagnostics", ["诊断", "診斷", "診断", "Diagnose", "Diagnóstico"]),
    ("Real-time spectrum", ["实时频谱", "即時頻譜", "リアルタイムスペクトラム", "Echtzeitspektrum", "Espectro en tiempo real"]),
    ("Waiting for signal", ["等待信号", "等待訊號", "信号待ち", "Warte auf Signal", "Esperando señal"]),
    ("EQ response", ["EQ 响应", "EQ 響應", "EQ レスポンス", "EQ-Frequenzgang", "Respuesta de EQ"]),
    ("AutoEq", ["AutoEq", "AutoEq", "AutoEq", "AutoEq", "AutoEq"]),
    ("Capability", ["设备支持情况", "裝置支援情況", "機器の対応状況", "Geräteunterstützung", "Compatibilidad del dispositivo"]),
    ("Correction", ["校正", "校正", "補正", "Korrektur", "Corrección"]),
    ("Dynamic EQ", ["动态均衡", "動態等化", "ダイナミック EQ", "Dynamik-EQ", "EQ dinámico"]),
    ("A/B", ["A/B", "A/B", "A/B", "A/B", "A/B"]),
    ("Genre", ["曲风", "曲風", "ジャンル", "Genre", "Género"]),
    ("Instruments", ["乐器", "樂器", "楽器", "Instrumente", "Instrumentos"]),
    ("Vocal", ["人声", "人聲", "ボーカル", "Gesang", "Voz"]),
    ("Theme source", ["配色依据", "配色依據", "配色の基準", "Farbgrundlage", "Criterio de color"]),
    ("Confidence", ["识别可信程度", "辨識可信程度", "認識の確かさ", "Erkennungssicherheit", "Confianza de la detección"]),
    ("Model", ["模型", "模型", "モデル", "Modell", "Modelo"]),
    ("Music context", ["音乐分析", "音樂分析", "音楽解析", "Musikanalyse", "Análisis musical"]),
    ("Current plan", ["当前方案", "目前方案", "現在のプラン", "Aktueller Plan", "Plan actual"]),
    ("Delta", ["变化", "變化", "変更量", "Änderung", "Cambio"]),
    ("Preview ready", ["预览就绪", "預覽就緒", "プレビュー準備完了", "Vorschau bereit", "Vista previa lista"]),
    ("Waiting for evidence", ["等待音频分析", "等待音訊分析", "音声解析を待っています", "Warte auf Audioanalyse", "Esperando análisis de audio"]),
    ("AI tuning", ["调音建议", "調音建議", "音質調整の提案", "Klangvorschläge", "Sugerencias de ajuste"]),
    ("Strips", ["通道", "通道", "ストリップ", "Kanäle", "Canales"]),
    ("Hardware I/O", ["音频设备", "音訊裝置", "音声デバイス", "Audiogeräte", "Dispositivos de audio"]),
    ("Mixer summary", ["混音摘要", "混音摘要", "ミキサー概要", "Mixer-Übersicht", "Resumen de mezcla"]),
    ("Revision", ["修订", "修訂", "リビジョン", "Revision", "Revisión"]),
    ("System health", ["运行状态", "執行狀態", "システム状態", "Systemzustand", "Estado del sistema"]),
    ("Theme", ["主题", "主題", "テーマ", "Theme", "Tema"]),
    ("Signal", ["信号", "訊號", "信号", "Signal", "Señal"]),
    ("Semantic", ["音乐识别", "音樂辨識", "音楽認識", "Musikerkennung", "Reconocimiento musical"]),
    ("Selected output", ["已选输出", "已選輸出", "選択中の出力", "Gewählter Ausgang", "Salida seleccionada"]),
    ("Active output", ["当前输出", "目前輸出", "現在の出力", "Aktiver Ausgang", "Salida activa"]),
    ("Output mode", ["输出模式", "輸出模式", "出力モード", "Ausgangsmodus", "Modo de salida"]),
    ("Profile key", ["设置名称", "設定名稱", "設定名", "Einstellungsname", "Nombre del perfil"]),
    ("Binding source", ["绑定来源", "綁定來源", "バインド元", "Bindungsquelle", "Origen del enlace"]),
    ("Reported latency", ["报告延迟", "回報延遲", "報告レイテンシ", "Gemeldete Latenz", "Latencia reportada"]),
    ("Rebinds", ["重新连接次数", "重新連接次數", "再接続回数", "Neuverbundene Ausgänge", "Reconexiones"]),
    ("Audio applications", ["音频应用", "音訊應用程式", "オーディオアプリ", "Audio-Anwendungen", "Aplicaciones de audio"]),
    ("Scope", ["范围", "範圍", "範囲", "Umfang", "Ámbito"]),
    ("Running", ["运行", "運行", "実行中", "Aktiv", "Activo"]),
    ("Application", ["应用", "應用程式", "アプリ", "Anwendung", "Aplicación"]),
    ("Application audio metadata is unavailable on this platform.", ["此平台无法获取应用音频元数据。", "此平台無法取得應用程式音訊中繼資料。", "このプラットフォームではアプリのオーディオ情報を取得できません。", "Anwendungs-Audiometadaten sind auf dieser Plattform nicht verfügbar.", "Los metadatos de audio de aplicaciones no están disponibles en esta plataforma."]),
    ("Confirm audio", ["确认音频", "確認音訊", "オーディオ確認", "Audio bestätigen", "Confirmar audio"]),
    ("Y confirms · any other key cancels", ["Y 确认 · 其他键取消", "Y 確認 · 其他按鍵取消", "Y で確認 · 他のキーでキャンセル", "Y bestätigt · andere Taste bricht ab", "Y confirma · otra tecla cancela"]),
    ("Changes", ["项调整", "項調整", "変更", "Änderungen", "cambios"]),
    ("Y apply · Esc discard · U undo after apply", ["Y 应用 · Esc 放弃 · 应用后 U 撤销", "Y 套用 · Esc 放棄 · 套用後 U 復原", "Y 適用 · Esc 破棄 · 適用後 U で元に戻す", "Y anwenden · Esc verwerfen · danach U rückgängig", "Y aplicar · Esc descartar · U deshacer después"]),
    ("AI preview", ["调音预览", "調音預覽", "調整プレビュー", "Klangvorschau", "Vista previa del ajuste"]),
    ("Shortcuts", ["快捷键", "快速鍵", "ショートカット", "Tastenkürzel", "Atajos"]),
    ("Dashboard: ↑/↓ select Sound · ←/→ adjust current device", ["总控台：↑/↓ 选择声音参数 · ←/→ 调整当前设备", "總控台：↑/↓ 選擇聲音參數 · ←/→ 調整目前裝置", "ダッシュボード：↑/↓ サウンド選択 · ←/→ 現在のデバイス調整", "Dashboard: ↑/↓ Klang wählen · ←/→ aktuelles Gerät anpassen", "Panel: ↑/↓ seleccionar sonido · ←/→ ajustar dispositivo actual"]),
    ("EQ Detail: ↑/↓ band · ←/→ gain · [/] Q · -/+ preamp", ["EQ 详情：↑/↓ 频段 · ←/→ 增益 · [/] Q · -/+ 前级", "EQ 詳情：↑/↓ 頻段 · ←/→ 增益 · [/] Q · -/+ 前級", "EQ 詳細：↑/↓ バンド · ←/→ ゲイン · [/] Q · -/+ プリアンプ", "EQ-Details: ↑/↓ Band · ←/→ Gain · [/] Q · -/+ Vorpegel", "Detalle EQ: ↑/↓ banda · ←/→ ganancia · [/] Q · -/+ preamp"]),
    ("Mixer: ↑/↓ application · Space toggle · Shift+A only · A all system audio", ["混音器：↑/↓ 选择应用 · 空格切换 · Shift+A 仅此应用 · A 返回全部系统音频", "混音器：↑/↓ 選擇應用程式 · 空白鍵切換 · Shift+A 僅此應用程式 · A 返回全部系統音訊", "ミキサー：↑/↓ アプリ選択 · Space 切替 · Shift+A このアプリのみ · A 全システム音声", "Mixer: ↑/↓ Anwendung · Leertaste umschalten · Shift+A nur diese · A gesamtes Systemaudio", "Mezclador: ↑/↓ aplicación · Espacio alternar · Shift+A solo esta · A todo el audio del sistema"]),
    ("O select output · Shift+O follow system default", ["O 选择输出 · Shift+O 跟随系统默认", "O 選擇輸出 · Shift+O 跟隨系統預設", "O 出力選択 · Shift+O システム既定を追従", "O Ausgang wählen · Shift+O Systemstandard folgen", "O elegir salida · Shift+O seguir predeterminada"]),
    ("P presets · B A/B · K save device · G/J/Y AI", ["P 预设 · B 对比 · K 保存设备 · G/J/Y 调音建议", "P 預設 · B 比較 · K 儲存裝置 · G/J/Y 調音建議", "P プリセット · B 比較 · K 保存 · G/J/Y 調整の提案", "P Presets · B Vergleich · K Speichern · G/J/Y Klangvorschläge", "P preajustes · B comparar · K guardar · G/J/Y sugerencias"]),
    ("Tab details · Esc dashboard · M compact · L language · S stop · Q close", ["Tab 详情 · Esc 总控台 · M 紧凑 · L 语言 · S 停止 · Q 关闭", "Tab 詳情 · Esc 總控台 · M 精簡 · L 語言 · S 停止 · Q 關閉", "Tab 詳細 · Esc ダッシュボード · M コンパクト · L 言語 · S 停止 · Q 閉じる", "Tab Details · Esc Dashboard · M kompakt · L Sprache · S Stopp · Q schließen", "Tab detalles · Esc panel · M compacto · L idioma · S detener · Q cerrar"]),
    ("↑↓ Sound · ←→ Adjust", ["↑↓ 声音 · ←→ 调整", "↑↓ 聲音 · ←→ 調整", "↑↓ サウンド · ←→ 調整", "↑↓ Klang · ←→ Anpassen", "↑↓ Sonido · ←→ Ajustar"]),
    ("↑↓ Band · ←→ Gain · [/] Q", ["↑↓ 频段 · ←→ 增益 · [/] Q", "↑↓ 頻段 · ←→ 增益 · [/] Q", "↑↓ バンド · ←→ ゲイン · [/] Q", "↑↓ Band · ←→ Gain · [/] Q", "↑↓ Banda · ←→ Ganancia · [/] Q"]),
    ("↑↓ App · Space Toggle · Shift+A Only · A All", ["↑↓ 应用 · 空格切换 · Shift+A 仅此 · A 全部", "↑↓ 應用程式 · 空白鍵切換 · Shift+A 僅此 · A 全部", "↑↓ アプリ · Space 切替 · Shift+A のみ · A 全体", "↑↓ App · Leertaste Umschalten · Shift+A Nur · A Alle", "↑↓ App · Espacio Alternar · Shift+A Solo · A Todo"]),
    ("Tab Next · Esc Dashboard", ["Tab 下一项 · Esc 总控台", "Tab 下一項 · Esc 總控台", "Tab 次へ · Esc ダッシュボード", "Tab Weiter · Esc Dashboard", "Tab siguiente · Esc panel"]),
    ("Help", ["帮助", "說明", "ヘルプ", "Hilfe", "Ayuda"]),
    ("Downloaded", ["已下载", "已下載", "ダウンロード済み", "Heruntergeladen", "Descargado"]),
    ("Integrated", ["已集成", "已整合", "統合済み", "Integriert", "Integrado"]),
    ("Researched", ["已研究", "已研究", "調査済み", "Untersucht", "Investigado"]),
    ("Yes", ["是", "是", "はい", "Ja", "Sí"]),
    ("No", ["否", "否", "いいえ", "Nein", "No"]),
    ("Now Playing", ["正在播放", "正在播放", "再生中", "Jetzt läuft", "Reproduciendo"]),
    ("Sound", ["声音", "聲音", "サウンド", "Sound", "Sonido"]),
    ("Mixer", ["混音", "混音", "ミキサー", "Mixer", "Mezclador"]),
    ("Devices", ["设备", "裝置", "デバイス", "Geräte", "Dispositivos"]),
    ("Intelligence", ["调音建议", "調音建議", "音質調整の提案", "Klangvorschläge", "Sugerencias de ajuste"]),
    ("System", ["系统", "系統", "システム", "System", "Sistema"]),
    ("Overview", ["总览", "總覽", "概要", "Übersicht", "Resumen"]),
    ("Effects", ["音效", "音效", "エフェクト", "Effekte", "Efectos"]),
    ("AI Tuning", ["调音建议", "調音建議", "音質調整の提案", "Klangvorschläge", "Sugerencias de ajuste"]),
    ("Settings", ["设置", "設定", "設定", "Einstellungen", "Ajustes"]),
    ("Input", ["输入", "輸入", "入力", "Eingang", "Entrada"]),
    ("Output", ["输出", "輸出", "出力", "Ausgang", "Salida"]),
    ("Sample rate", ["采样率", "取樣率", "サンプルレート", "Abtastrate", "Frecuencia de muestreo"]),
    ("Engine", ["引擎", "引擎", "エンジン", "Engine", "Motor"]),
    ("Current preset", ["当前预设", "目前預設", "現在のプリセット", "Aktuelles Preset", "Preajuste actual"]),
    ("State", ["状态", "狀態", "状態", "Status", "Estado"]),
    ("Signal path", ["信号路径", "訊號路徑", "信号経路", "Signalweg", "Ruta de señal"]),
    ("Live status", ["实时状态", "即時狀態", "ライブ状態", "Live-Status", "Estado en vivo"]),
    ("Unavailable", ["不可用", "不可用", "利用不可", "Nicht verfügbar", "No disponible"]),
    ("Available", ["可用", "可用", "利用可能", "Verfügbar", "Disponible"]),
    ("Underruns", ["音频缓冲不足", "音訊緩衝不足", "音声バッファ不足", "Audiopuffer-Lücken", "Falta de audio en búfer"]),
    ("Overruns", ["音频缓冲溢出", "音訊緩衝溢位", "音声バッファ超過", "Audiopuffer-Überläufe", "Desbordamientos de audio"]),
    ("Adaptive reduction", ["自适应衰减", "自適應衰減", "適応リダクション", "Adaptive Absenkung", "Reducción adaptativa"]),
    ("Source", ["来源", "來源", "ソース", "Quelle", "Fuente"]),
    ("Meter", ["电平表", "電平表", "メーター", "Pegel", "Medidor"]),
    ("Mixer strips", ["混音通道", "混音通道", "ミキサーチャンネル", "Mixer-Kanäle", "Canales de mezcla"]),
    ("Matrix engine", ["混音处理", "混音處理", "ミキシング", "Mischung", "Mezcla de audio"]),
    ("Hardware multi-input", ["硬件多输入", "硬體多輸入", "ハードウェア多入力", "Hardware-Mehrfacheingang", "Multi-entrada de hardware"]),
    ("Dual bus engine", ["两路独立混音", "兩路獨立混音", "2 系統のミックス", "Zwei getrennte Mischungen", "Dos mezclas independientes"]),
    ("Hardware multi-output", ["硬件多输出", "硬體多輸出", "ハードウェア多出力", "Hardware-Mehrfachausgang", "Multi-salida de hardware"]),
    ("Ducking", ["自动压低背景音", "自動壓低背景音", "ダッキング", "Ducking", "Ducking"]),
    ("Per-strip DSP", ["各通道音效", "各通道音效", "チャンネル別の音質調整", "Effekte je Kanal", "Efectos por canal"]),
    ("Mixer capability", ["混音能力", "混音能力", "ミキサー機能", "Mixer-Fähigkeiten", "Capacidades de mezcla"]),
    ("ID", ["ID", "ID", "ID", "ID", "ID"]),
    ("Direction", ["方向", "方向", "方向", "Richtung", "Dirección"]),
    ("Device", ["设备", "裝置", "デバイス", "Gerät", "Dispositivo"]),
    ("Default", ["默认", "預設", "既定", "Standard", "Predeterminado"]),
    ("Goal", ["目标", "目標", "目標", "Ziel", "Objetivo"]),
    ("Planner", ["调音建议", "調音建議", "調整の提案", "Klangvorschläge", "Sugerencias de ajuste"]),
    ("Language", ["语言", "語言", "言語", "Sprache", "Idioma"]),
    ("Backend", ["后端", "後端", "バックエンド", "Backend", "Backend"]),
    ("Clock bridge", ["音频同步", "音訊同步", "音声同期", "Audiosynchronisierung", "Sincronización de audio"]),
    ("Preset catalog", ["预设库", "預設庫", "プリセット一覧", "Preset-Katalog", "Catálogo de preajustes"]),
    ("Category", ["分类", "分類", "カテゴリ", "Kategorie", "Categoría"]),
    ("Name", ["名称", "名稱", "名前", "Name", "Nombre"]),
    ("Stable ID", ["稳定 ID", "穩定 ID", "安定 ID", "Stabile ID", "ID estable"]),
    ("Safe preamp", ["安全前级", "安全前級", "安全プリアンプ", "Sicherer Vorpegel", "Preamp seguro"]),
    ("Bandwidth", ["带宽", "頻寬", "帯域幅", "Bandbreite", "Ancho de banda"]),
    ("No preset available", ["无可用预设", "無可用預設", "利用可能なプリセットなし", "Kein Preset verfügbar", "No hay preajustes disponibles"]),
    ("Preset details", ["预设详情", "預設詳情", "プリセット詳細", "Preset-Details", "Detalles del preajuste"]),
    ("Live spectrum", ["实时频谱", "即時頻譜", "ライブスペクトラム", "Live-Spektrum", "Espectro en vivo"]),
    ("Playback", ["播放路径", "播放路徑", "再生", "Wiedergabe", "Reproducción"]),
    ("Output level", ["输出电平", "輸出電平", "出力レベル", "Ausgangspegel", "Nivel de salida"]),
    ("Headroom", ["增益余量", "增益餘量", "ヘッドルーム", "Headroom", "Margen"]),
    ("Loudness", ["响度", "響度", "ラウドネス", "Lautheit", "Sonoridad"]),
    ("Health", ["运行状态", "執行狀態", "動作状態", "Systemzustand", "Estado"]),
    ("Device profile", ["设备配置", "裝置設定檔", "デバイスプロファイル", "Geräteprofil", "Perfil del dispositivo"]),
    ("AutoEq model", ["AutoEq 型号", "AutoEq 型號", "AutoEq モデル", "AutoEq-Modell", "Modelo AutoEq"]),
    ("Match confidence", ["匹配置信度", "匹配信心度", "一致信頼度", "Treffsicherheit", "Confianza de coincidencia"]),
    ("Physical limits", ["设备频率范围", "裝置頻率範圍", "機器の周波数範囲", "Frequenzbereich des Geräts", "Rango de frecuencias del dispositivo"]),
    ("Virtual bass", ["虚拟低频", "虛擬低頻", "バーチャルベース", "Virtueller Bass", "Graves virtuales"]),
    ("Bass Assist", ["虚拟低音", "虛擬低音", "仮想低音", "Virtueller Bass", "Graves virtuales"]),
    ("Unknown", ["未知", "未知", "不明", "Unbekannt", "Desconocido"]),
    ("Unknown limits are never guessed from the music signal.", ["音乐分析不能测出耳机或音箱的频率范围。", "音樂分析無法測出耳機或喇叭的頻率範圍。", "音楽の解析では機器の周波数範囲は測れません。", "Musikanalyse misst nicht den Frequenzbereich des Geräts.", "Analizar música no mide el rango de frecuencias del dispositivo."]),
    ("Inputs", ["输入设备", "輸入裝置", "入力デバイス", "Eingänge", "Entradas"]),
    ("Outputs", ["输出设备", "輸出裝置", "出力デバイス", "Ausgänge", "Salidas"]),
    ("Signal analysis", ["信号分析", "訊號分析", "信号解析", "Signalanalyse", "Análisis de señal"]),
    ("Models", ["模型", "模型", "モデル", "Modelle", "Modelos"]),
    ("RMS", ["RMS", "RMS", "RMS", "RMS", "RMS"]),
    ("Crest", ["峰值与平均差", "峰值與平均差", "ピークと平均の差", "Spitze minus Mittel", "Pico menos media"]),
    ("Momentary LUFS", ["瞬时响度", "瞬時響度", "モーメンタリー LUFS", "Momentane LUFS", "LUFS momentáneo"]),
    ("Short-term LUFS", ["短时响度", "短時響度", "ショートターム LUFS", "Kurzzeit-LUFS", "LUFS a corto plazo"]),
    ("True peak", ["真实峰值", "真實峰值", "トゥルーピーク", "True Peak", "Pico verdadero"]),
    ("Stereo correlation", ["左右声道相似度", "左右聲道相似度", "左右音声の類似度", "Ähnlichkeit links/rechts", "Similitud izquierda/derecha"]),
    ("Ready", ["可用", "可用", "使用可能", "Bereit", "Listo"]),
    ("Candidate", ["候选", "候選", "候補", "Kandidat", "Candidato"]),
    ("Research", ["研究中", "研究中", "調査中", "Forschung", "Investigación"]),
    ("Models never run on the real-time audio callback.", ["模型绝不会运行在实时音频回调线程。", "模型絕不會執行在即時音訊回呼執行緒。", "モデルはリアルタイム音声コールバックでは実行しません。", "Modelle laufen niemals im Echtzeit-Audio-Callback.", "Los modelos nunca se ejecutan en el callback de audio en tiempo real."]),
    ("INPUT SPECTRUM · PRE-EQ", ["输入频谱 · 均衡前", "輸入頻譜 · 等化前", "入力スペクトラム · EQ 前", "EINGANGSSPEKTRUM · VOR EQ", "ESPECTRO DE ENTRADA · PRE-EQ"]),
    ("No live signal", ["暂无实时信号", "暫無即時訊號", "ライブ信号なし", "Kein Live-Signal", "Sin señal en vivo"]),
    ("Agents read capabilities and revisions before proposing changes.", ["Agent 会先读取能力与版本，再提出修改方案。", "Agent 會先讀取能力與版本，再提出修改方案。", "エージェントは変更提案の前に機能とリビジョンを確認します。", "Agenten lesen Fähigkeiten und Revisionen vor Änderungsvorschlägen.", "Los agentes leen capacidades y revisiones antes de proponer cambios."]),
    ("Preview, apply and undo use the same validation path as manual controls.", ["预览、应用和撤销与手动控制共用同一校验路径。", "預覽、套用和復原與手動控制共用同一驗證路徑。", "プレビュー・適用・元に戻すは手動操作と同じ検証経路を使います。", "Vorschau, Anwenden und Rückgängig nutzen dieselbe Validierung wie manuelle Bedienung.", "Vista previa, aplicar y deshacer usan la misma validación que los controles manuales."]),
    ("No agent tool can execute arbitrary shell commands through Maris.", ["任何 Agent 工具都不能通过 Maris 执行任意 Shell 命令。", "任何 Agent 工具都不能透過 Maris 執行任意 Shell 命令。", "Maris 経由で任意のシェルコマンドを実行できるエージェントツールはありません。", "Kein Agent-Werkzeug kann über Maris beliebige Shell-Befehle ausführen.", "Ninguna herramienta de agente puede ejecutar comandos de shell arbitrarios mediante Maris."]),
    ("Per-strip meters appear only after a real multi-input backend is connected.", ["只有接入真实多输入后端后才显示逐通道电平。", "只有接入真實多輸入後端後才顯示逐通道電平。", "実際の多入力バックエンド接続後にのみチャンネル別メーターを表示します。", "Kanalpegel erscheinen erst nach Anschluss eines echten Multi-Input-Backends.", "Los medidores por canal solo aparecen tras conectar un backend multi-entrada real."]),
    ("L cycles the UI language immediately.", ["按 L 会立即切换界面语言。", "按 L 會立即切換介面語言。", "L で UI 言語を即時切り替えます。", "L wechselt die UI-Sprache sofort.", "L cambia inmediatamente el idioma de la interfaz."]),
    ("Device names and technical identifiers are never translated.", ["设备名称和技术标识始终保留原文。", "裝置名稱和技術識別始終保留原文。", "デバイス名と技術識別子は翻訳しません。", "Gerätenamen und technische Kennungen werden nicht übersetzt.", "Los nombres de dispositivos e identificadores técnicos no se traducen."]),
    (
        "Compact monitor",
        [
            "迷你监视器",
            "迷你監視器",
            "ミニモニター",
            "Mini-Monitor",
            "Monitor compacto",
        ],
    ),
    (
        "Presets",
        [
            "音色预设",
            "音色預設",
            "プリセット",
            "Presets",
            "Preajustes",
        ],
    ),
    ("Expand", ["展开", "展開", "展開", "Öffnen", "Ampliar"]),
    ("Stop", ["停止", "停止", "停止", "Stoppen", "Detener"]),
    ("Close", ["关闭", "關閉", "閉じる", "Schließen", "Cerrar"]),
    (
        "Stop requested",
        [
            "正在停止处理",
            "正在停止處理",
            "処理を停止中",
            "Verarbeitung wird beendet",
            "Deteniendo procesamiento",
        ],
    ),
    (
        "All effects bypassed · Space to enable",
        [
            "全部音效已旁路 · 空格开启",
            "全部音效已旁路 · 空白鍵開啟",
            "全効果バイパス · Spaceで有効化",
            "Alle Effekte umgangen · Leertaste aktiviert",
            "Efectos omitidos · Espacio para activar",
        ],
    ),
    ("STANDBY", ["待机", "待機", "待機", "BEREIT", "EN ESPERA"]),
    (
        "LIVE",
        ["实时处理", "即時處理", "処理中", "LIVE", "EN VIVO"],
    ),
    (
        "EQ BYPASS",
        [
            "均衡器旁路",
            "等化器旁路",
            "EQ バイパス",
            "EQ BYPASS",
            "EQ OMITIDO",
        ],
    ),
    (
        "VOICE AI",
        ["语音降噪", "語音降噪", "音声ノイズ低減", "Sprachentrauschung", "Reducción de ruido de voz"],
    ),
    (
        "Shape your sound.",
        [
            "塑造你的声音。",
            "塑造你的聲音。",
            "音を、あなたらしく。",
            "Gestalte deinen Klang.",
            "Da forma a tu sonido.",
        ],
    ),
    (
        "EQ SHAPE · calculated, not a spectrum",
        [
            "均衡器响应 · 非实测频谱",
            "等化器響應 · 非實測頻譜",
            "EQ カーブ · 計算値",
            "EQ-KURVE · berechnet",
            "CURVA EQ · calculada",
        ],
    ),
    (
        "PARAMETRIC EQ · arrows to shape",
        [
            "参数均衡器 · 方向键调整",
            "參數等化器 · 方向鍵調整",
            "パラメトリック EQ · 矢印で調整",
            "PARAMETRISCHER EQ",
            "ECUALIZADOR PARAMÉTRICO",
        ],
    ),
    ("BAND", ["频段", "頻段", "バンド", "BAND", "BANDA"]),
    (
        "FREQUENCY",
        ["频率", "頻率", "周波数", "FREQUENZ", "FRECUENCIA"],
    ),
    (
        "GAIN dB",
        ["增益 dB", "增益 dB", "ゲイン dB", "PEGEL dB", "GANANCIA dB"],
    ),
    (
        "SIGNAL ROUTE",
        [
            "音频路径",
            "音訊路徑",
            "信号経路",
            "SIGNALWEG",
            "RUTA DE AUDIO",
        ],
    ),
    (
        "OUTPUT · dBFS",
        [
            "输出电平 · dBFS",
            "輸出電平 · dBFS",
            "出力 · dBFS",
            "AUSGANG · dBFS",
            "SALIDA · dBFS",
        ],
    ),
    (
        "SMART LAB",
        [
            "调音建议",
            "調音建議",
            "音質調整の提案",
            "Klangvorschläge",
            "Sugerencias de ajuste",
        ],
    ),
    ("IN", ["输入", "輸入", "入力", "EINGANG", "ENTRADA"]),
    ("OUT", ["输出", "輸出", "出力", "AUSGANG", "SALIDA"]),
    ("FORMAT", ["格式", "格式", "形式", "FORMAT", "FORMATO"]),
    (
        "HEADROOM",
        ["增益余量", "增益餘量", "ヘッドルーム", "HEADROOM", "MARGEN"],
    ),
    (
        "DROPOUTS",
        ["音频中断", "音訊中斷", "音声の途切れ", "AUSSETZER", "CORTES"],
    ),
    ("GOAL", ["目标", "目標", "目標", "ZIEL", "OBJETIVO"]),
    (
        "PLANNER",
        ["调音建议", "調音建議", "音質調整の提案", "Klangvorschläge", "Sugerencias de ajuste"],
    ),
    ("MODEL", ["模型", "模型", "モデル", "MODELL", "MODELO"]),
    (
        "CROSSFEED",
        [
            "交叉馈送",
            "交叉饋送",
            "クロスフィード",
            "CROSSFEED",
            "CRUCE",
        ],
    ),
    (
        "WIDTH",
        ["声场宽度", "音場寬度", "音場幅", "BREITE", "AMPLITUD"],
    ),
    (
        "local heuristic",
        [
            "本机规则",
            "本機規則",
            "ローカルルール",
            "Lokale Regeln",
            "Reglas locales",
        ],
    ),
    (
        "off / music safe",
        [
            "关闭 / 音乐模式",
            "關閉 / 音樂模式",
            "オフ / 音楽用",
            "aus / Musikmodus",
            "apagado / música",
        ],
    ),
    (
        "System audio (automatic)",
        [
            "系统声音（自动）",
            "系統聲音（自動）",
            "システム音声（自動）",
            "Systemaudio (automatisch)",
            "Audio del sistema (automático)",
        ],
    ),
    (
        "System default",
        [
            "系统默认",
            "系統預設",
            "システム既定",
            "Systemstandard",
            "Predeterminado",
        ],
    ),
    (
        "Open tuning console",
        [
            "打开调音台",
            "開啟調音台",
            "調整コンソールを開く",
            "Konsole öffnen",
            "Abrir consola",
        ],
    ),
    (
        "Stop processing",
        [
            "停止处理",
            "停止處理",
            "処理を停止",
            "Verarbeitung stoppen",
            "Detener procesamiento",
        ],
    ),
    (
        "Enable system audio tuning",
        [
            "开启系统调音",
            "開啟系統調音",
            "システム調整を開始",
            "Systemaudio aktivieren",
            "Activar ajuste del sistema",
        ],
    ),
    (
        "Quit Maris",
        [
            "退出 Maris",
            "結束 Maris",
            "Maris を終了",
            "Maris beenden",
            "Salir de Maris",
        ],
    ),
    (
        "Enable tonal processing",
        [
            "开启音色处理",
            "開啟音色處理",
            "音色処理を有効化",
            "Klangbearbeitung aktivieren",
            "Activar procesamiento tonal",
        ],
    ),
    (
        "Bypass tonal processing",
        [
            "旁路音色处理",
            "旁路音色處理",
            "音色処理をバイパス",
            "Klangbearbeitung umgehen",
            "Omitir procesamiento tonal",
        ],
    ),
    ("Standby", ["待机", "待機", "待機", "Bereit", "En espera"]),
    (
        "Processing",
        ["处理中", "處理中", "処理中", "Verarbeitung", "Procesando"],
    ),
    (
        "Awaiting authorization",
        [
            "等待系统授权",
            "等待系統授權",
            "許可待ち",
            "Warte auf Erlaubnis",
            "Esperando permiso",
        ],
    ),
    (
        "Audio unavailable",
        [
            "音频不可用",
            "音訊無法使用",
            "音声を利用できません",
            "Audio nicht verfügbar",
            "Audio no disponible",
        ],
    ),
    (
        "EQ bypassed",
        [
            "均衡器已旁路",
            "等化器已旁路",
            "EQ バイパス",
            "EQ umgangen",
            "EQ omitido",
        ],
    ),
    (
        "Voice AI",
        ["语音降噪", "語音降噪", "音声ノイズ低減", "Sprachentrauschung", "Reducción de ruido de voz"],
    ),
    (
        "MUSIC ENHANCEMENT",
        [
            "音乐增强",
            "音樂增強",
            "音楽エンハンス",
            "MUSIKVERBESSERUNG",
            "MEJORA MUSICAL",
        ],
    ),
    ("Bass", ["低音", "低音", "低音", "Bass", "Graves"]),
    (
        "Presence",
        ["清晰度", "清晰度", "明瞭さ", "Präsenz", "Presencia"],
    ),
    ("Air", ["高音", "高音", "高音", "Höhen", "Agudos"]),
    (
        "Softness",
        ["柔和度", "柔和度", "柔らかさ", "Sanftheit", "Suavidad"],
    ),
    (
        "Intensity",
        ["音效强度", "音效強度", "効果の強さ", "Effektstärke", "Intensidad del efecto"],
    ),
    (
        "Adaptive EQ",
        [
            "自适应动态均衡",
            "自適應動態等化",
            "アダプティブ EQ",
            "Adaptiver EQ",
            "EQ adaptativo",
        ],
    ),
    (
        "Stereo width",
        [
            "立体声宽度",
            "立體聲寬度",
            "ステレオの広がり",
            "Stereobreite",
            "Ancho estéreo",
        ],
    ),
    (
        "Balance",
        ["左右平衡", "左右平衡", "左右バランス", "Balance", "Balance"],
    ),
    (
        "Compressor",
        [
            "动态压缩",
            "動態壓縮",
            "コンプレッサー",
            "Kompressor",
            "Compresor",
        ],
    ),
    (
        "Reference A",
        ["参考 A", "參考 A", "基準 A", "Referenz A", "Referencia A"],
    ),
    (
        "Enhanced B",
        ["增强 B", "增強 B", "効果 B", "Verbessert B", "Mejora B"],
    ),
    (
        "Device profile saved",
        [
            "已保存当前设备音色",
            "已儲存目前裝置音色",
            "デバイス設定を保存しました",
            "Geräteprofil gespeichert",
            "Perfil de dispositivo guardado",
        ],
    ),
    (
        "No measured correction",
        [
            "未加载测量校正",
            "未載入測量校正",
            "測定補正なし",
            "Keine Messkorrektur",
            "Sin corrección medida",
        ],
    ),
    (
        "English fallback for detailed diagnostics",
        [
            "详细诊断暂用英文",
            "詳細診斷暫用英文",
            "詳細診断は英語で表示",
            "Detaildiagnosen auf Englisch",
            "Diagnósticos detallados en inglés",
        ],
    ),
    (
        "E enhancement · B compare · K save device · L language",
        [
            "E 音乐增强 · B 对比 · K 保存设备 · L 语言",
            "E 音樂增強 · B 對比 · K 儲存裝置 · L 語言",
            "E 効果 · B 比較 · K 保存 · L 言語",
            "E Effekte · B Vergleich · K Speichern · L Sprache",
            "E efectos · B comparar · K guardar · L idioma",
        ],
    ),
    (
        "Up/Down select · Left/Right adjust · B compare · K save · Esc back",
        [
            "上下选择 · 左右调整 · B 对比 · K 保存 · Esc 返回",
            "上下選擇 · 左右調整 · B 對比 · K 儲存 · Esc 返回",
            "上下で選択 · 左右で調整 · B 比較 · K 保存 · Esc 戻る",
            "Auf/Ab wählen · Links/Rechts ändern · B vergleichen · K speichern · Esc zurück",
            "Arriba/abajo elegir · Izquierda/derecha ajustar · B comparar · K guardar · Esc volver",
        ],
    ),
];
