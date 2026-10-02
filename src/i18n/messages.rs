//! Localized presentation of notices, planner explanations and expected diagnostics.
//! This never rewrites proposals, errors on the wire, user names or state identifiers.
#[rustfmt::skip]
pub const TRANSLATIONS: &[(&str, [&str; 5])] = &[
    ("Goal: {goal} · Changes: {count}", ["目标：{goal} · 变更：{count}", "目標：{goal} · 變更：{count}", "目標：{goal} · 変更：{count}", "Ziel: {goal} · Änderungen: {count}", "Objetivo: {goal} · Cambios: {count}"]),
    ("Changes: {count}", ["变更：{count}", "變更：{count}", "変更：{count}", "Änderungen: {count}", "Cambios: {count}"]),
    ("Preset applied: {preset}", ["已应用预设：{preset}", "已套用預設：{preset}", "プリセット適用：{preset}", "Preset angewendet: {preset}", "Preajuste aplicado: {preset}"]),
    ("Applied: {action}", ["已完成：{action}", "已完成：{action}", "適用済み：{action}", "Angewendet: {action}", "Aplicado: {action}"]),
    ("AI goal: {goal}", ["调音目标：{goal}", "調音目標：{goal}", "調整目標：{goal}", "Hörziel: {goal}", "Objetivo: {goal}"]),
    ("Pending app scope: {count} selected.", ["待应用范围：已选 {count} 个应用。", "待套用範圍：已選 {count} 個應用。", "選択待機：{count} アプリ。", "Auswahl ausstehend: {count} Apps.", "Ámbito pendiente: {count} apps."]),
    ("Applying {count} selected application(s).", ["正在应用所选的 {count} 个应用。", "正在套用所選的 {count} 個應用。", "{count} アプリを適用中。", "{count} ausgewählte Apps werden angewendet.", "Aplicando {count} apps seleccionadas."]),
    ("Output request queued: {device}.", ["已提交输出切换：{device}。", "已提交輸出切換：{device}。", "出力切替を受付：{device}。", "Ausgangswechsel vorgemerkt: {device}.", "Cambio de salida solicitado: {device}."]),
    ("Starting on {device}.", ["正在连接：{device}。", "正在連接：{device}。", "接続中：{device}。", "Start auf {device}.", "Iniciando en {device}."]),
    ("Tray unavailable: {error}. Console controls remain available.", ["菜单栏不可用：{error}。调音台仍可操作。", "選單列不可用：{error}。調音台仍可操作。", "トレイ利用不可：{error}。コンソール操作は利用可能です。", "Tray nicht verfügbar: {error}. Die Konsole bleibt nutzbar.", "Bandeja no disponible: {error}. La consola sigue disponible."]),
    ("Audio start failed. Press Enter to retry.", ["音频启动失败。按 Enter 重试。", "音訊啟動失敗。按 Enter 重試。", "音声開始失敗。Enter で再試行。", "Audiostart fehlgeschlagen. Enter versucht erneut.", "No se pudo iniciar audio. Enter reintenta."]),
    ("Language changed.", ["语言已切换。", "語言已切換。", "言語を切り替えました。", "Sprache geändert.", "Idioma cambiado."]),
    ("Stop requested.", ["已请求停止。", "已要求停止。", "停止を要求しました。", "Stopp angefordert.", "Parada solicitada."]),
    ("Previous change undone.", ["已撤销上一次修改。", "已復原上一次修改。", "直前の変更を戻しました。", "Letzte Änderung rückgängig gemacht.", "Último cambio deshecho."]),
    ("Tuning applied. B compares; U undoes it.", ["调音已应用。B 对比，U 撤销。", "調音已套用。B 比較，U 復原。", "調整適用済み。B 比較、U 戻す。", "Tuning angewendet. B vergleicht; U macht rückgängig.", "Ajuste aplicado. B compara; U deshace."]),
    ("No adjustment needed; the last undo remains available.", ["无需调整；仍可撤销上一次修改。", "無需調整；仍可復原上一次修改。", "調整不要。直前の変更はまだ戻せます。", "Keine Anpassung nötig; letztes Rückgängig bleibt verfügbar.", "No hace falta ajustar; el último cambio sigue siendo reversible."]),
    ("Starting native system audio.", ["正在启动系统音频处理。", "正在啟動系統音訊處理。", "ネイティブシステム音声を開始中。", "Natives Systemaudio startet.", "Iniciando audio nativo del sistema."]),
    ("Pinned output disappeared; Maris returned to system-default following.", ["固定输出已断开；Maris 已恢复跟随系统默认输出。", "固定輸出已中斷；Maris 已恢復跟隨系統預設輸出。", "固定出力が切断され、システム既定に戻りました。", "Fester Ausgang getrennt; Maris folgt wieder dem Systemstandard.", "Salida fija desconectada; Maris vuelve a la predeterminada."]),
    ("Output request queued: follow system default.", ["已提交请求：跟随系统默认输出。", "已提交要求：跟隨系統預設輸出。", "システム既定出力への追従を受付。", "Systemstandard als Ausgang angefordert.", "Solicitado seguir la salida predeterminada."]),
    ("Starting on the system default output.", ["正在连接系统默认输出。", "正在連接系統預設輸出。", "システム既定出力で開始中。", "Start auf dem Systemstandardausgang.", "Iniciando en la salida predeterminada."]),
    ("Returning to all system playback except Maris.", ["正在恢复全部系统播放，排除 Maris 自身。", "正在恢復全部系統播放，排除 Maris 本身。", "Maris 自身を除く全システム再生に戻しています。", "Zurück zur gesamten Wiedergabe außer Maris.", "Volviendo a toda la reproducción excepto Maris."]),
    ("All system playback selected; Enter applies, Esc returns without switching.", ["已选择全部系统播放；Enter 应用，Esc 返回且不切换。", "已選擇全部系統播放；Enter 套用，Esc 返回且不切換。", "全システム再生を選択。Enter 適用、Esc は変更せず戻る。", "Systemaudio ausgewählt; Enter anwenden, Esc ohne Wechsel zurück.", "Todo el sistema seleccionado; Enter aplica, Esc vuelve sin cambiar."]),
    ("Enlarge the terminal to adjust sound; O opens the output picker.", ["请放大终端调整音色；O 可打开输出选择器。", "請放大終端調整音色；O 可開啟輸出選擇器。", "音色調整は端末を拡大。O で出力選択。", "Zum Einstellen Terminal vergrößern; O öffnet die Ausgangswahl.", "Amplía la terminal para ajustar; O abre las salidas."]),
    ("Audio control failed.", ["音频操作失败。", "音訊操作失敗。", "音声操作に失敗しました。", "Audiosteuerung fehlgeschlagen.", "Falló el control de audio."]),
    ("No active audio session", ["当前没有运行音频处理", "目前沒有執行音訊處理", "実行中の音声セッションなし", "Keine aktive Audiositzung", "No hay sesión de audio activa"]),
    ("Analysis is warming up", ["正在收集音频分析数据", "正在收集音訊分析資料", "音声解析の準備中", "Audioanalyse läuft an", "Preparando el análisis de audio"]),
    ("Analysis is stale", ["分析数据已过期", "分析資料已過期", "解析結果が古くなっています", "Analyse ist veraltet", "Análisis obsoleto"]),
    ("Analysis is stale or from the future", ["分析时间无效，请等待新的音频数据", "分析時間無效，請等待新的音訊資料", "解析時刻が無効です。新しいデータを待ってください", "Analysezeit ungültig; auf neue Daten warten", "Hora de análisis inválida; espera nuevos datos"]),
    ("Analysis sample rate does not match the active output", ["分析采样率与当前输出不一致，请等待重新分析", "分析取樣率與目前輸出不一致，請等待重新分析", "解析と出力のサンプルレート不一致", "Abtastrate der Analyse passt nicht zum Ausgang", "La frecuencia del análisis no coincide con la salida"]),
    ("At least two seconds of evidence are required", ["需要至少两秒的有效音频数据", "需要至少兩秒的有效音訊資料", "2 秒以上の音声データが必要です", "Mindestens zwei Sekunden Audiodaten nötig", "Se necesitan al menos dos segundos de audio"]),
    ("Signal evidence is incomplete or too quiet", ["音频数据不完整或信号过弱", "音訊資料不完整或訊號過弱", "音声データ不足または音量が小さすぎます", "Audiodaten unvollständig oder zu leise", "Señal incompleta o demasiado tenue"]),
    ("No previous listening profile to restore", ["没有可撤销的调音修改", "沒有可復原的調音修改", "戻せる調整履歴がありません", "Keine Hörprofiländerung zum Rückgängigmachen", "No hay ajuste de escucha que deshacer"]),
    ("No previous profile to restore", ["没有可恢复的配置", "沒有可還原的設定檔", "復元可能な設定なし", "Kein vorheriges Profil vorhanden", "No hay perfil anterior"]),
    ("Tuning proposal expired or has a future timestamp", ["调音预览已过期或时间无效，请重新预览", "調音預覽已過期或時間無效，請重新預覽", "プレビューの期限切れまたは時刻無効。再作成してください", "Vorschau abgelaufen oder Zeit ungültig; neu erstellen", "Vista previa caducada o con hora inválida; regénérala"]),
    ("Active output changed after preview", ["预览后输出设备已改变，请重新预览", "預覽後輸出裝置已改變，請重新預覽", "出力が変わりました。再度プレビューしてください", "Ausgang seit Vorschau geändert; erneut prüfen", "La salida cambió tras la vista previa; repítela"]),
    ("Device profile binding changed after preview", ["设备使用的设置已改变，请重新预览", "裝置使用的設定已變更，請重新預覽", "デバイス設定の対応が変わりました", "Gerätebindung seit Vorschau geändert", "Cambió el enlace del perfil del dispositivo"]),
    ("Listening profile changed after preview", ["调音参数已改变，请重新预览", "調音參數已改變，請重新預覽", "調整値が変更されました。再確認してください", "Hörprofil seit Vorschau geändert", "El perfil de escucha cambió tras la vista previa"]),
    ("Device preference changed after preview", ["设备的音色设置已改变，请重新预览", "裝置的音色設定已變更，請重新預覽", "デバイスの好み設定が変わりました", "Gerätepräferenz seit Vorschau geändert", "La preferencia del dispositivo ha cambiado"]),
    ("Device capability changed after preview", ["设备支持的调整范围已改变，请重新预览", "裝置支援的調整範圍已變更，請重新預覽", "デバイス制限が変わりました", "Gerätegrenzen seit Vorschau geändert", "Han cambiado los límites del dispositivo"]),
    ("Tuning proposal was modified", ["预览内容被修改，已拒绝应用", "預覽內容被修改，已拒絕套用", "プレビューの改変を検出し適用を拒否", "Veränderte Vorschau abgelehnt", "Se rechazó una vista previa modificada"]),
    ("Live output switching requires native system audio", ["实时切换输出需要原生系统音频会话", "即時切換輸出需要原生系統音訊工作階段", "出力切替にはネイティブシステム音声が必要です", "Live-Ausgangswechsel benötigt natives Systemaudio", "Cambiar salida requiere audio nativo del sistema"]),
    ("Application capture requires native system audio", ["应用音频采集需要原生系统音频会话", "應用音訊擷取需要原生系統音訊工作階段", "アプリ音声にはネイティブシステム音声が必要です", "App-Aufnahme benötigt natives Systemaudio", "La captura de apps requiere audio nativo del sistema"]),
    ("A selected application exited; update the selection before applying", ["所选应用已退出，请更新选择后再应用", "所選應用已結束，請更新選擇後再套用", "選択アプリが終了しました。選び直してください", "Eine App wurde beendet; Auswahl vor Anwendung aktualisieren", "Una app seleccionada se cerró; actualiza la selección"]),
    ("Audio startup failed", ["音频启动失败", "音訊啟動失敗", "音声開始失敗", "Audiostart fehlgeschlagen", "No se pudo iniciar audio"]),
    ("Applying a tuning proposal must make the changed branch audible.", ["应用后切到调整后的音色，方便对比。", "套用後切到調整後的音色，方便比較。", "変更を聴けるよう効果側に切り替えます。", "Zum Hören der Änderung wird der bearbeitete Zweig gewählt.", "Se selecciona la señal procesada para escuchar el cambio."]),
    ("Measured low-band energy is high ({value}); avoid adding a bass shelf and favor bounded dynamic control.", ["低频能量占比较高（{value}），不再增加低音，改为适当压低过强的低频。", "低頻能量占比較高（{value}），不再增加低音，改為適當壓低過強的低頻。", "低域エネルギーが高い（{value}）ため低音を追加せず、制限付き動的制御を使います。", "Hohe gemessene Bassenergie ({value}); kein weiterer Bass-Shelf, begrenzte Dynamikregelung.", "Energía grave alta ({value}); sin realce adicional y con control dinámico limitado."]),
    ("Known limited low-frequency extension favors bounded virtual-bass harmonics over a large sub-bass boost.", ["设备不擅长播放很低的声音；建议少量使用虚拟低音，不大幅增加超低音。", "裝置不擅長播放很低的聲音；建議少量使用虛擬低音，不大幅增加超低音。", "低域の限界が確認されているため、超低域増幅より控えめな倍音を使います。", "Bekannte Bassgrenze: begrenzte virtuelle Obertöne statt starker Tiefbassanhebung.", "Con extensión grave limitada, se prefieren armónicos controlados a realzar subgraves."]),
    ("Low measured bass energy, a bound correction profile, and available digital headroom permit a small preference boost.", ["低频较弱，设备已有校正设置，音量也留有余量；建议小幅增加低音。", "低頻較弱，裝置已有校正設定，音量也留有餘量；建議小幅增加低音。", "低音が少なく補正と余裕があるため、小幅な低音調整が可能です。", "Wenig Bass, gebundene Korrektur und Headroom erlauben eine kleine Anhebung.", "Poco grave, corrección y margen digital permiten un realce pequeño."]),
    ("Bass boost withheld because device correction/headroom or physical low-frequency capability is not established.", ["校正、余量或设备低频能力尚不明确，不增加低频增益。", "校正、餘量或裝置低頻能力尚不明確，不增加低頻增益。", "補正・余裕・低域能力が未確認のため低音を増幅しません。", "Keine Bassanhebung ohne bestätigte Korrektur, Headroom oder Bassfähigkeit.", "No se realzan graves sin corrección, margen o capacidad confirmados."]),
    ("Measured treble density plus bright-instrument semantic context favors bounded dynamic de-harshing rather than static high-frequency boost.", ["高频偏强，识别到的乐器也偏明亮；建议适当减少刺耳的高频，不再增加高音。", "高頻偏強，辨識到的樂器也偏明亮；建議適當減少刺耳的高頻，不再增加高音。", "高域密度と明るい楽器の情報から、高域追加より制限付き動的抑制を選びます。", "Hochtondichte und helle Instrumente sprechen für begrenzte dynamische Absenkung.", "Agudos densos e instrumentos brillantes favorecen atenuación dinámica limitada."]),
    ("Measured treble energy plus a sufficiently confident soft/calm semantic mood favors a small bounded softness increase.", ["音乐被识别为偏柔和，但高频较强；建议稍微柔化高音。", "音樂被辨識為偏柔和，但高頻較強；建議稍微柔化高音。", "高域エネルギーと穏やかな曲調の根拠から柔らかさを少し加えます。", "Hochtonenergie und ruhige Stimmung erlauben etwas mehr Sanftheit.", "La energía aguda y un contexto tranquilo permiten aumentar un poco la suavidad."]),
    ("Treble density/loudness evidence favors bounded dynamic de-harshing rather than static high-frequency boost.", ["高频偏强或声音偏响；建议压低刺耳的部分，不再增加高音。", "高頻偏強或聲音偏響；建議壓低刺耳的部分，不再增加高音。", "高域密度や音量から、高域追加より制限付き動的抑制を使います。", "Hochtondichte/Lautheit sprechen für begrenzte dynamische Absenkung statt Anhebung.", "Los agudos y la sonoridad favorecen control dinámico limitado, no otro realce."]),
    ("Confident vocal semantic context plus low measured presence supports a small bounded presence preference boost.", ["检测到人声，但相关频段偏弱；建议小幅提高清晰度。", "偵測到人聲，但相關頻段偏弱；建議小幅提高清晰度。", "確かな歌声情報と低いプレゼンスから明瞭さを少し上げます。", "Gesangskontext und geringe Präsenz erlauben eine kleine Präsenzanhebung.", "El contexto vocal y la baja presencia permiten un pequeño realce de claridad."]),
    ("Low stereo correlation blocks additional widening and moves the preference toward unity width.", ["左右声道差异已经较大；不再加宽，逐步恢复原有的立体声宽度。", "左右聲道差異已經較大；不再加寬，逐步恢復原有的立體聲寬度。", "相関が低いため拡幅せず、元の幅に近づけます。", "Geringe Stereokorrelation: nicht weiter verbreitern, Richtung Originalbreite.", "Baja correlación estéreo: sin más amplitud y acercándose a la original."]),
    ("Already low-crest material does not justify enabling another compressor; retain the user's explicit compression choice.", ["素材动态已经偏低，不自动追加压缩；保留用户的压缩选择。", "素材動態已經偏低，不自動追加壓縮；保留使用者的壓縮選擇。", "既に低ダイナミクスのため圧縮を追加せず、ユーザー設定を維持します。", "Bereits geringe Dynamik: keinen weiteren Kompressor automatisch aktivieren.", "El material ya tiene poca dinámica; se conserva la compresión elegida por el usuario."]),
    ("Device capability limits reduced one or more requested preference boosts.", ["部分提升已按设备能力上限缩减。", "部分提升已依裝置能力上限縮減。", "デバイス制限により一部の増幅を抑えました。", "Gerätegrenzen haben angeforderte Anhebungen reduziert.", "Los límites del dispositivo redujeron algunos realces solicitados."]),
    ("Subtle bass and treble cuts for background listening; no compression.", ["轻微削减低频和高频，适合背景聆听；不压缩动态。", "輕微削減低頻和高頻，適合背景聆聽；不壓縮動態。", "低音・高音を控えめにし背景用に。圧縮なし。", "Leichte Bass-/Höhenabsenkung für Hintergrundmusik; ohne Kompression.", "Recortes suaves de graves y agudos para música de fondo; sin compresión."]),
    ("A softer high end and bounded dynamic cuts; not a hearing-protection mode.", ["让高音柔和一些，并压低突出的声音；不能保护听力。", "讓高音柔和一些，並壓低突出的聲音；不能保護聽力。", "高域を和らげ限定的に動的制御。聴覚保護ではありません。", "Sanftere Höhen und begrenzte Dynamikabsenkung; kein Gehörschutz.", "Agudos suaves y cortes dinámicos limitados; no protege la audición."]),
    ("Less bass and a small presence lift for dialogue and podcasts; no denoising.", ["减少低频并小幅提高清晰度，适合对白和播客；不做降噪。", "減少低頻並小幅提高清晰度，適合對白和播客；不做降噪。", "会話・ポッドキャスト用に低音を抑え明瞭さを少し追加。ノイズ除去なし。", "Weniger Bass, etwas Präsenz für Dialog/Podcast; keine Entrauschung.", "Menos graves y más presencia para diálogo y pódcast; sin reducción de ruido."]),
    ("Modest weight and dialogue presence with original stereo dynamics; not surround sound.", ["适量厚度和对白清晰度，保持原始立体声动态；不是环绕声。", "適量厚度和對白清晰度，保持原始立體聲動態；不是環繞聲。", "厚みと会話の明瞭さを少し追加。元のステレオ動態を維持し、サラウンドではありません。", "Etwas Fülle und Dialogpräsenz bei ursprünglicher Dynamik; kein Surround.", "Cuerpo y presencia moderados con dinámica estéreo original; no es envolvente."]),
    ("Reduces bass and loud peaks for late-night dialogue; enables compression without makeup gain.", ["减少低音，压低较响的对白，适合夜间使用；不会自动提高整体音量。", "減少低音，壓低較響的對白，適合夜間使用；不會自動提高整體音量。", "夜の会話用に低音と大きなピークを抑制。補償ゲインなしの圧縮を有効化。", "Weniger Bass und laute Spitzen für Nachtdialog; Kompression ohne Makeup-Gain.", "Reduce graves y picos para diálogo nocturno; compresión sin ganancia de compensación."]),
    ("A restrained presence and air lift for acoustic recordings; keeps compression off.", ["小幅提升清晰度和空气感，适合原声录音；保持压缩关闭。", "小幅提高清晰度和空氣感，適合原聲錄音；保持壓縮關閉。", "生楽器録音に控えめな明瞭さと空気感。圧縮なし。", "Dezente Präsenz und Brillanz für Akustikaufnahmen; keine Kompression.", "Presencia y brillo moderados para grabaciones acústicas; sin compresión."]),
    ("Neutral static tone and very light dynamic control; preserves stereo width and dynamics.", ["静态音色中性，动态控制轻微；保持原始声场宽度与动态。", "靜態音色中性，動態控制輕微；保持原始音場寬度與動態。", "静的 EQ は中立、動的制御はごく軽め。元の幅とダイナミクスを維持。", "Neutraler statischer Klang, sehr leichte Dynamikregelung; Originalbreite bleibt.", "Tono estático neutro y control dinámico leve; conserva amplitud y dinámica."]),
    ("Cuts excess bass preference and strengthens bounded dynamic control; never adds a bass boost.", ["减少低音，并加强对过强低频的抑制；不会额外增加低音。", "減少低音，並加強對過強低頻的抑制；不會額外增加低音。", "余分な低音を抑え、制限付き動的制御を強化。低音増幅なし。", "Weniger Bass mit stärkerer begrenzter Dynamikregelung; keine Bassanhebung.", "Recorta graves excesivos y refuerza control limitado; nunca añade realce de graves."]),
    ("Less bass masking with a small presence lift; no positional-audio or surround claim.", ["减少低频遮蔽，小幅提高清晰度；不声称位置增强或环绕声。", "減少低頻遮蔽，小幅提高清晰度；不宣稱位置增強或環繞聲。", "低音を抑え明瞭さを追加。定位強化やサラウンドの保証なし。", "Weniger Bassmaskierung, etwas Präsenz; keine Ortungs- oder Surround-Zusage.", "Menos enmascaramiento grave y más presencia; sin promesas de posicionamiento o envolvente."]),
    ("No sub-bass boost; mild virtual bass only when device capability and bass limits are known.", ["不提升超低频；只有设备能力和低频边界明确时才启用轻微虚拟低音。", "不提升超低頻；只有裝置能力和低頻邊界明確時才啟用輕微虛擬低音。", "超低音は増幅せず、確認済みの機器能力と低域限界がある時だけ軽い仮想低音を使用。", "Kein Tiefbassboost; virtueller Bass nur bei bekannten Gerätegrenzen.", "Sin realce de subgraves; graves virtuales solo con capacidad y límites conocidos."]),
];

/// Named placeholders are substituted once; argument text is never interpreted as a template.
pub fn format_for(language: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut remaining = super::for_language(language, key);
    let mut result = String::new();
    while let Some(start) = remaining.find('{') {
        result.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find('}').map(|end| start + end) else {
            result.push_str(&remaining[start..]);
            return result;
        };
        let name = &remaining[start + 1..end];
        if let Some((_, value)) = args.iter().find(|(candidate, _)| *candidate == name) {
            result.push_str(value);
        } else {
            result.push_str(&remaining[start..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    result.push_str(remaining);
    result
}
pub fn format(key: &str, args: &[(&str, &str)]) -> String {
    format_for(super::code(), key, args)
}

pub fn diagnostic_for(language: &str, raw: &str) -> String {
    if super::has_translation(raw) {
        return super::for_language(language, raw).to_owned();
    }
    // An anyhow context is useful in translation; keep the original cause for diagnosis.
    if let Some((context, cause)) = raw.split_once(": ") {
        if super::has_translation(context) {
            return format!("{}: {cause}", super::for_language(language, context));
        }
    }
    if super::normalize(language) == 0 {
        raw.to_owned()
    } else {
        format!(
            "{}: {raw}",
            super::for_language(language, "Technical details")
        )
    }
}
pub fn diagnostic(raw: &str) -> String {
    diagnostic_for(super::code(), raw)
}

pub fn change(raw: &str) -> String {
    let Some((label, values)) = raw.split_once(": ") else {
        return raw.into();
    };
    let Some((before, after)) = values.split_once(" -> ") else {
        return raw.into();
    };
    format!(
        "{}: {} → {}",
        super::text(label),
        super::text(before),
        super::text(after)
    )
}
pub fn reason(raw: &str) -> String {
    let start = "Measured low-band energy is high (";
    let end = "); avoid adding a bass shelf and favor bounded dynamic control.";
    if let Some(value) = raw
        .strip_prefix(start)
        .and_then(|text| text.strip_suffix(end))
    {
        if value
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            return format("Measured low-band energy is high ({value}); avoid adding a bass shelf and favor bounded dynamic control.", &[("value", value)]);
        }
    }
    super::text(raw).to_owned()
}

#[derive(Clone, Debug)]
pub struct Notice {
    key: String,
    args: Vec<(String, String, bool)>,
    diagnostic: bool,
}
impl From<&str> for Notice {
    fn from(key: &str) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
            diagnostic: false,
        }
    }
}
impl Notice {
    pub fn new(key: &str) -> Self {
        key.into()
    }
    pub fn arg(mut self, name: &str, value: impl ToString) -> Self {
        self.args.push((name.into(), value.to_string(), false));
        self
    }
    pub fn label_arg(mut self, name: &str, key: &str) -> Self {
        self.args.push((name.into(), key.into(), true));
        self
    }
    pub fn error(error: impl std::fmt::Display) -> Self {
        Self {
            key: error.to_string(),
            args: Vec::new(),
            diagnostic: true,
        }
    }
    pub fn for_language(&self, language: &str) -> String {
        if self.diagnostic {
            return diagnostic_for(language, &self.key);
        }
        let args: Vec<_> = self
            .args
            .iter()
            .map(|(name, value, localized)| {
                (
                    name.as_str(),
                    if *localized {
                        super::for_language(language, value)
                    } else {
                        value
                    },
                )
            })
            .collect();
        format_for(language, &self.key, &args)
    }
    pub fn render(&self) -> String {
        self.for_language(super::code())
    }
}
