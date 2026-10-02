//! Localized labels for the rewritten console. English identifiers stay unchanged.
pub const TRANSLATIONS: &[(&str, [&str; 5])] = &[
    (
        "Assist",
        ["调音助手", "調音助手", "アシスト", "Assistent", "Asistente"],
    ),
    (
        "Listening Assist",
        [
            "聆听调音助手",
            "聆聽調音助手",
            "リスニングアシスト",
            "Hörassistent",
            "Asistente de escucha",
        ],
    ),
    (
        "Choose a goal and preview",
        [
            "选择听感目标，预览后再应用",
            "選擇聽感目標，預覽後再套用",
            "目標を選び適用前に確認",
            "Hörziel wählen und Vorschau öffnen",
            "Elige un objetivo y previsualiza",
        ],
    ),
    (
        "Balanced",
        ["均衡", "均衡", "バランス", "Ausgewogen", "Equilibrado"],
    ),
    ("Warm", ["温暖", "溫暖", "温かみ", "Warm", "Cálido"]),
    ("Clear", ["清晰", "清晰", "明瞭", "Klar", "Claro"]),
    ("Soft", ["柔和", "柔和", "柔らかさ", "Sanft", "Suave"]),
    (
        "Ready for preview",
        [
            "可生成调音预览",
            "可產生調音預覽",
            "プレビュー準備完了",
            "Bereit zur Vorschau",
            "Listo para vista previa",
        ],
    ),
    (
        "Built-in processing",
        [
            "内置处理",
            "內建處理",
            "内蔵処理",
            "Integrierte Verarbeitung",
            "Procesamiento integrado",
        ],
    ),
    (
        "No model download needed",
        [
            "无需下载模型",
            "無需下載模型",
            "モデル取得不要",
            "Kein Modelldownload nötig",
            "Sin descargar modelos",
        ],
    ),
    (
        "Speech only",
        [
            "仅语音降噪",
            "僅語音降噪",
            "音声のみ",
            "Nur Sprache",
            "Solo voz",
        ],
    ),
    (
        "Included",
        ["已内置", "已內建", "内蔵済み", "Enthalten", "Incluido"],
    ),
    (
        "Not included",
        [
            "未包含",
            "未包含",
            "未搭載",
            "Nicht enthalten",
            "No incluido",
        ],
    ),
    (
        "Bass energy",
        [
            "低频能量",
            "低頻能量",
            "低音エネルギー",
            "Bassenergie",
            "Energía de graves",
        ],
    ),
    (
        "Treble energy",
        [
            "高频能量",
            "高頻能量",
            "高音エネルギー",
            "Hochtonenergie",
            "Energía de agudos",
        ],
    ),
    (
        "Local signal-guided tuning",
        [
            "根据当前音乐提出调音建议",
            "根據目前音樂提出調音建議",
            "再生中の音楽に合わせた調整案",
            "Klangvorschläge zur laufenden Musik",
            "Ajustes sugeridos para la música actual",
        ],
    ),
    (
        "Preview before apply · U restores the last change",
        [
            "预览后再应用 · U 撤销最近修改",
            "預覽後再套用 · U 復原最近修改",
            "適用前に確認 · U で直前の変更を戻す",
            "Erst Vorschau · U macht die letzte Änderung rückgängig",
            "Previsualiza antes de aplicar · U deshace el último cambio",
        ],
    ),
    (
        "B Compare",
        ["B 对比", "B 比較", "B 比較", "B Vergleichen", "B Comparar"],
    ),
    (
        "U Undo",
        [
            "U 撤销",
            "U 復原",
            "U 元に戻す",
            "U Rückgängig",
            "U Deshacer",
        ],
    ),
    (
        "Models · ↑↓ scroll",
        [
            "模型 · ↑↓ 滚动",
            "模型 · ↑↓ 捲動",
            "モデル · ↑↓ スクロール",
            "Modelle · ↑↓ scrollen",
            "Modelos · ↑↓ desplazar",
        ],
    ),
    ("Studio", ["总控", "總控", "スタジオ", "Studio", "Estudio"]),
    ("EQ", ["EQ", "EQ", "EQ", "EQ", "EQ"]),
    ("Apps", ["应用", "應用", "アプリ", "Apps", "Apps"]),
    (
        "AI / Models",
        [
            "AI / 模型",
            "AI / 模型",
            "AI / モデル",
            "KI / Modelle",
            "IA / Modelos",
        ],
    ),
    (
        "Select a band above",
        [
            "点击上方频段",
            "點選上方頻段",
            "上のバンドを選択",
            "Band oben wählen",
            "Elige una banda arriba",
        ],
    ),
    (
        "Application routing",
        [
            "应用声音输出",
            "應用聲音輸出",
            "アプリ音声ルーティング",
            "Anwendungsrouting",
            "Enrutamiento de apps",
        ],
    ),
    (
        "Selected applications",
        [
            "所选应用",
            "所選應用",
            "選択アプリ",
            "Gewählte Apps",
            "Apps seleccionadas",
        ],
    ),
    (
        "System playback",
        [
            "系统播放",
            "系統播放",
            "システム再生",
            "Systemwiedergabe",
            "Reproducción del sistema",
        ],
    ),
    (
        "Mark a scope, then apply once",
        [
            "勾选范围后确认应用",
            "勾選範圍後確認套用",
            "範囲を選び一度適用",
            "Bereich markieren, dann anwenden",
            "Marca el ámbito y aplica",
        ],
    ),
    (
        "Pending selection",
        [
            "待应用选择",
            "待套用選擇",
            "選択待機",
            "Ausstehende Auswahl",
            "Selección pendiente",
        ],
    ),
    (
        "A All playback",
        [
            "A 全部播放",
            "A 全部播放",
            "A 全再生",
            "A Alle Wiedergabe",
            "A Toda reproducción",
        ],
    ),
    (
        "Enter Apply scope",
        [
            "Enter 应用选择",
            "Enter 套用選擇",
            "Enter 範囲を適用",
            "Enter Bereich anwenden",
            "Enter Aplicar ámbito",
        ],
    ),
    (
        "Esc Studio",
        [
            "Esc 返回总控",
            "Esc 返回總控",
            "Esc スタジオ",
            "Esc Studio",
            "Esc Estudio",
        ],
    ),
    (
        "Output route",
        [
            "输出路径",
            "輸出路徑",
            "出力経路",
            "Ausgangsroute",
            "Ruta de salida",
        ],
    ),
    (
        "Device correction",
        [
            "设备校正",
            "裝置校正",
            "デバイス補正",
            "Gerätekorrektur",
            "Corrección del dispositivo",
        ],
    ),
    (
        "Device identity",
        [
            "设备信息",
            "裝置資訊",
            "デバイス識別",
            "Geräteidentität",
            "Identidad del dispositivo",
        ],
    ),
    (
        "O Choose output",
        [
            "O 选择输出",
            "O 選擇輸出",
            "O 出力選択",
            "O Ausgang wählen",
            "O Elegir salida",
        ],
    ),
    (
        "Selection does not change system volume",
        [
            "选择不会更改系统音量",
            "選擇不會變更系統音量",
            "選択でシステム音量は変わりません",
            "Auswahl ändert keine Systemlautstärke",
            "Elegir no cambia el volumen del sistema",
        ],
    ),
    (
        "Preview required",
        [
            "尚未生成预览",
            "尚未產生預覽",
            "プレビュー未作成",
            "Vorschau erforderlich",
            "Vista previa necesaria",
        ],
    ),
    (
        "Cached",
        ["已缓存", "已快取", "保存済み", "Im Cache", "En caché"],
    ),
    (
        "Not cached",
        ["未缓存", "未快取", "未保存", "Nicht im Cache", "Sin caché"],
    ),
    (
        "Cached weights are not an inference runtime",
        [
            "权重已缓存不等于推理可用",
            "權重已快取不等於推論可用",
            "重み保存と推論対応は別です",
            "Gewichte im Cache sind keine Inferenzlaufzeit",
            "Pesos en caché no equivalen a inferencia",
        ],
    ),
    (
        "G Goal",
        ["G 目标", "G 目標", "G 目標", "G Ziel", "G Objetivo"],
    ),
    (
        "J Preview tuning",
        [
            "J 预览调音",
            "J 預覽調音",
            "J 調整プレビュー",
            "J Tuning-Vorschau",
            "J Vista previa",
        ],
    ),
    (
        "D Download MusicNN",
        [
            "D 下载 MusicNN",
            "D 下載 MusicNN",
            "D MusicNN 取得",
            "D MusicNN laden",
            "D Descargar MusicNN",
        ],
    ),
    (
        "Engine health",
        [
            "引擎运行状态",
            "引擎執行狀態",
            "エンジン状態",
            "Engine-Zustand",
            "Estado del motor",
        ],
    ),
    (
        "Capture",
        ["采集范围", "擷取範圍", "キャプチャ", "Aufnahme", "Captura"],
    ),
    (
        "Callback budget",
        [
            "回调时间占比",
            "回呼時間占比",
            "コールバック比率",
            "Callback-Budget",
            "Presupuesto callback",
        ],
    ),
    (
        "Callback maximum",
        [
            "最长回调时间",
            "最長回呼時間",
            "最大コールバック",
            "Callback-Maximum",
            "Máximo callback",
        ],
    ),
    (
        "Analysis queue",
        [
            "分析队列",
            "分析佇列",
            "解析キュー",
            "Analysewarteschlange",
            "Cola de análisis",
        ],
    ),
    (
        "Applied revision",
        [
            "已应用版本",
            "已套用版本",
            "適用リビジョン",
            "Angewandte Revision",
            "Revisión aplicada",
        ],
    ),
    (
        "Listening revision",
        [
            "调音版本",
            "調音版本",
            "調整リビジョン",
            "Hörprofil-Revision",
            "Revisión de escucha",
        ],
    ),
    (
        "Last audio command",
        [
            "最近音频操作",
            "最近音訊操作",
            "直近の音声操作",
            "Letzter Audiobefehl",
            "Última orden de audio",
        ],
    ),
    (
        "Callback budget is wall time, not CPU usage",
        [
            "回调占比是耗时比例，不是 CPU 使用率",
            "回呼占比是耗時比例，不是 CPU 使用率",
            "コールバック比率は経過時間で CPU 使用率ではありません",
            "Callback-Budget ist Zeitanteil, nicht CPU-Auslastung",
            "El presupuesto mide tiempo, no uso de CPU",
        ],
    ),
    (
        "Click / ↑↓ select · ←→ adjust",
        [
            "点击 / ↑↓ 选择 · ←→ 调整",
            "點選 / ↑↓ 選擇 · ←→ 調整",
            "クリック / ↑↓ 選択 · ←→ 調整",
            "Klick / ↑↓ wählen · ←→ ändern",
            "Clic / ↑↓ elegir · ←→ ajustar",
        ],
    ),
    (
        "Awaiting telemetry",
        [
            "等待实时状态",
            "等待即時狀態",
            "状態待機中",
            "Warte auf Audiostatus",
            "Esperando estado de audio",
        ],
    ),
    (
        "STALE",
        ["状态过期", "狀態過期", "更新なし", "VERALTET", "OBSOLETO"],
    ),
    (
        "Resize to expand",
        [
            "放大终端展开",
            "放大終端展開",
            "端末拡大で表示",
            "Terminal vergrößern",
            "Amplía la terminal",
        ],
    ),
    (
        "Saved offline",
        [
            "已保存 · 未运行",
            "已儲存 · 未執行",
            "保存済み · 停止中",
            "Offline gespeichert",
            "Guardado sin conexión",
        ],
    ),
    (
        "Apply state unknown",
        [
            "应用状态未知",
            "套用狀態未知",
            "適用状態不明",
            "Anwendungsstatus unbekannt",
            "Estado de aplicación desconocido",
        ],
    ),
    (
        "Pending audio update",
        [
            "等待音频应用",
            "等待音訊套用",
            "オーディオ適用待ち",
            "Audio-Aktualisierung ausstehend",
            "Actualización de audio pendiente",
        ],
    ),
    (
        "Applied",
        ["已应用", "已套用", "適用済み", "Angewendet", "Aplicado"],
    ),
    (
        "Downloading MusicNN",
        [
            "正在下载 MusicNN",
            "正在下載 MusicNN",
            "MusicNN を取得中",
            "MusicNN wird geladen",
            "Descargando MusicNN",
        ],
    ),
    (
        "Click sound · −/+ adjust · wheel selects",
        [
            "点击参数 · −/+ 调整 · 滚轮选择",
            "點選參數 · −/+ 調整 · 滾輪選擇",
            "クリックで選択 · −/+ 調整 · ホイール選択",
            "Klang anklicken · −/+ ändern · Rad wählen",
            "Clic sonido · −/+ ajustar · rueda elige",
        ],
    ),
    (
        "No correction",
        [
            "未校正",
            "未校正",
            "補正なし",
            "Keine Korrektur",
            "Sin corrección",
        ],
    ),
    (
        "Limits",
        ["设备限制", "裝置限制", "制限", "Grenzen", "Límites"],
    ),
    (
        "Pinned output",
        [
            "固定输出",
            "固定輸出",
            "固定出力",
            "Fester Ausgang",
            "Salida fija",
        ],
    ),
    (
        "Profile EQ",
        [
            "配置 EQ",
            "設定 EQ",
            "プロファイル EQ",
            "Profil-EQ",
            "EQ de perfil",
        ],
    ),
    (
        "Inference",
        [
            "模型推理",
            "模型推論",
            "モデル推論",
            "Modellinferenz",
            "Inferencia",
        ],
    ),
    (
        "P preset · J AI · U undo · Tab details · ? help",
        [
            "P 预设 · J 调音建议 · U 撤销 · Tab 详情 · ? 帮助",
            "P 預設 · J 調音建議 · U 復原 · Tab 詳情 · ? 說明",
            "P プリセット · J 調整案 · U 元に戻す · Tab 詳細 · ? ヘルプ",
            "P Preset · J Vorschläge · U rückgängig · Tab Details · ? Hilfe",
            "P preajuste · J sugerencias · U deshacer · Tab detalles · ? ayuda",
        ],
    ),
    (
        "Detail views",
        [
            "详情视图",
            "詳情檢視",
            "詳細ビュー",
            "Detailansichten",
            "Vistas de detalle",
        ],
    ),
    ("NOW", ["总控", "總控", "現在", "JETZT", "AHORA"]),
    ("SOUND", ["音色", "音色", "サウンド", "KLANG", "SONIDO"]),
    ("APPS", ["应用", "應用程式", "アプリ", "APPS", "APPS"]),
    ("SYSTEM", ["系统", "系統", "システム", "SYSTEM", "SISTEMA"]),
    ("OUTPUT", ["输出", "輸出", "出力", "AUSGANG", "SALIDA"]),
    ("LEFT", ["左声道", "左聲道", "左", "LINKS", "IZQUIERDA"]),
    ("RIGHT", ["右声道", "右聲道", "右", "RECHTS", "DERECHA"]),
    ("STATE", ["状态", "狀態", "状態", "ZUSTAND", "ESTADO"]),
    ("STATUS", ["状态", "狀態", "状態", "STATUS", "ESTADO"]),
    ("CACHE", ["缓存", "快取", "キャッシュ", "CACHE", "CACHÉ"]),
    (
        "PRESET",
        ["预设", "預設", "プリセット", "PRESET", "PREAJUSTE"],
    ),
    ("GROUP", ["分组", "群組", "グループ", "GRUPPE", "GRUPO"]),
    ("SOURCE", ["来源", "來源", "ソース", "QUELLE", "ORIGEN"]),
    ("MODE", ["模式", "模式", "モード", "MODUS", "MODO"]),
    (
        "APPLICATION",
        ["应用", "應用程式", "アプリ", "ANWENDUNG", "APLICACIÓN"],
    ),
    (
        "Preamp",
        [
            "前级增益",
            "前級增益",
            "プリアンプ",
            "Vorverstärkung",
            "Preamplificador",
        ],
    ),
    (
        "EQ bypass",
        [
            "EQ 旁路",
            "EQ 旁路",
            "EQ バイパス",
            "EQ-Bypass",
            "Omitir EQ",
        ],
    ),
    (
        "Crossfeed",
        [
            "声道混合",
            "聲道混合",
            "クロスフィード",
            "Crossfeed",
            "Cruce estéreo",
        ],
    ),
    (
        "EQ width",
        [
            "EQ 声场宽度",
            "EQ 音場寬度",
            "EQ 幅",
            "EQ-Breite",
            "Amplitud EQ",
        ],
    ),
    (
        "Music processing",
        [
            "音乐处理",
            "音樂處理",
            "音楽処理",
            "Musikverarbeitung",
            "Procesamiento musical",
        ],
    ),
    (
        "Level match",
        [
            "响度匹配",
            "響度匹配",
            "音量合わせ",
            "Pegelabgleich",
            "Igualar nivel",
        ],
    ),
    (
        "No audio applications",
        [
            "暂无音频应用",
            "暫無音訊應用程式",
            "オーディオアプリなし",
            "Keine Audio-Anwendungen",
            "Sin aplicaciones de audio",
        ],
    ),
    (
        "LIVE SPECTRUM · PRE-EQ",
        [
            "实时频谱 · EQ 前",
            "即時頻譜 · EQ 前",
            "ライブスペクトラム · EQ 前",
            "LIVE-SPEKTRUM · VOR EQ",
            "ESPECTRO EN VIVO · PRE-EQ",
        ],
    ),
    (
        "EQ SHAPE · calculated",
        [
            "EQ 曲线 · 计算值",
            "EQ 曲線 · 計算值",
            "EQ カーブ · 計算値",
            "EQ-KURVE · berechnet",
            "CURVA EQ · calculada",
        ],
    ),
    (
        "QUICK SOUND · ↑↓ choose · ←→ adjust",
        [
            "快捷调音 · ↑↓ 选择 · ←→ 调整",
            "快速調音 · ↑↓ 選擇 · ←→ 調整",
            "簡単調整 · ↑↓ 選択 · ←→ 調整",
            "KLANG · ↑↓ wählen · ←→ ändern",
            "SONIDO · ↑↓ elegir · ←→ ajustar",
        ],
    ),
    (
        "SOUND · one cursor, one adjustment",
        [
            "音色 · 选择参数后直接调整",
            "音色 · 選擇參數後直接調整",
            "サウンド · 選択して調整",
            "KLANG · wählen und anpassen",
            "SONIDO · elegir y ajustar",
        ],
    ),
    (
        "MUSIC CONTEXT / AI",
        [
            "音乐分析 / 调音建议",
            "音樂分析 / 調音建議",
            "音楽解析 / 調整の提案",
            "MUSIKANALYSE / KLANGVORSCHLÄGE",
            "ANÁLISIS MUSICAL / AJUSTES",
        ],
    ),
    (
        "AUDIO ENGINE",
        [
            "音频引擎",
            "音訊引擎",
            "オーディオエンジン",
            "AUDIO-ENGINE",
            "MOTOR DE AUDIO",
        ],
    ),
    (
        "DEVICE IDENTITY",
        [
            "设备信息",
            "裝置資訊",
            "デバイス識別",
            "GERÄTEIDENTITÄT",
            "IDENTIDAD DEL DISPOSITIVO",
        ],
    ),
    (
        "MODELS · D downloads verified MusicNN weights",
        [
            "模型 · D 下载并校验 MusicNN 权重",
            "模型 · D 下載並驗證 MusicNN 權重",
            "モデル · D で MusicNN を取得・検証",
            "MODELLE · D lädt geprüfte MusicNN-Gewichte",
            "MODELOS · D descarga y verifica MusicNN",
        ],
    ),
    (
        "O opens output picker · no switching until Enter",
        [
            "O 选择输出 · Enter 确认后才切换",
            "O 選擇輸出 · Enter 確認後才切換",
            "O 出力選択 · Enter で切替",
            "O Ausgang wählen · Enter schaltet um",
            "O elegir salida · Enter cambia",
        ],
    ),
    (
        "Follow system default",
        [
            "跟随系统默认",
            "跟隨系統預設",
            "システム既定に追従",
            "Systemstandard folgen",
            "Seguir salida predeterminada",
        ],
    ),
    (
        "OUTPUT · Enter switches · Esc cancels",
        [
            "输出 · Enter 切换 · Esc 取消",
            "輸出 · Enter 切換 · Esc 取消",
            "出力 · Enter 切替 · Esc 中止",
            "AUSGANG · Enter wechseln · Esc abbrechen",
            "SALIDA · Enter cambia · Esc cancela",
        ],
    ),
    (
        "PRESETS · Enter applies · Esc cancels",
        [
            "预设 · Enter 应用 · Esc 取消",
            "預設 · Enter 套用 · Esc 取消",
            "プリセット · Enter 適用 · Esc 中止",
            "PRESETS · Enter anwenden · Esc abbrechen",
            "PREAJUSTES · Enter aplica · Esc cancela",
        ],
    ),
    (
        "Enter confirms · Esc cancels",
        [
            "Enter 确认 · Esc 取消",
            "Enter 確認 · Esc 取消",
            "Enter 確認 · Esc 中止",
            "Enter bestätigt · Esc bricht ab",
            "Enter confirma · Esc cancela",
        ],
    ),
    (
        "APPS · Space select · Enter apply",
        [
            "应用 · 空格选择 · Enter 应用",
            "應用程式 · 空白鍵選擇 · Enter 套用",
            "アプリ · Space 選択 · Enter 適用",
            "APPS · Leertaste wählen · Enter anwenden",
            "APPS · Espacio elige · Enter aplica",
        ],
    ),
    (
        "CAPTURE SCOPE",
        [
            "采集范围",
            "擷取範圍",
            "キャプチャ範囲",
            "AUFNAHMEBEREICH",
            "ÁMBITO DE CAPTURA",
        ],
    ),
    (
        "↑↓ choose sound · ←→ adjust",
        [
            "↑↓ 选择音色 · ←→ 调整",
            "↑↓ 選擇音色 · ←→ 調整",
            "↑↓ サウンド選択 · ←→ 調整",
            "↑↓ Klang wählen · ←→ ändern",
            "↑↓ elegir sonido · ←→ ajustar",
        ],
    ),
    (
        "↑↓ choose · ←→ adjust · [/] Q · Space A/B",
        [
            "↑↓ 选择 · ←→ 调整 · [/] Q · 空格 A/B",
            "↑↓ 選擇 · ←→ 調整 · [/] Q · 空白鍵 A/B",
            "↑↓ 選択 · ←→ 調整 · [/] Q · Space A/B",
            "↑↓ wählen · ←→ ändern · [/] Q · Leertaste A/B",
            "↑↓ elegir · ←→ ajustar · [/] Q · Espacio A/B",
        ],
    ),
    (
        "↑↓ choose · Space mark · A mark all · Enter apply",
        [
            "↑↓ 选择 · 空格标记 · A 全部 · Enter 应用",
            "↑↓ 選擇 · 空白鍵標記 · A 全部 · Enter 套用",
            "↑↓ 選択 · Space マーク · A 全体 · Enter 適用",
            "↑↓ wählen · Leertaste markieren · A alle · Enter anwenden",
            "↑↓ elegir · Espacio marcar · A todos · Enter aplicar",
        ],
    ),
    (
        "D download MusicNN · diagnostics are read-only",
        [
            "D 下载 MusicNN · 诊断信息只读",
            "D 下載 MusicNN · 診斷資訊唯讀",
            "D MusicNN 取得 · 診断は読み取り専用",
            "D MusicNN laden · Diagnose schreibgeschützt",
            "D descargar MusicNN · diagnóstico de solo lectura",
        ],
    ),
    (
        "O output",
        ["O 输出", "O 輸出", "O 出力", "O Ausgang", "O salida"],
    ),
    (
        "P preset · J AI · U undo · ? help · Q close",
        [
            "P 预设 · J 调音建议 · U 撤销 · ? 帮助 · Q 关闭",
            "P 預設 · J 調音建議 · U 復原 · ? 說明 · Q 關閉",
            "P プリセット · J 調整案 · U 元に戻す · ? ヘルプ · Q 閉じる",
            "P Preset · J Vorschläge · U rückgängig · ? Hilfe · Q schließen",
            "P preajuste · J sugerencias · U deshacer · ? ayuda · Q cerrar",
        ],
    ),
    (
        "Enlarge terminal to confirm. Esc cancels.",
        [
            "请扩大终端后确认。Esc 取消。",
            "請放大終端後確認。Esc 取消。",
            "端末を広げて確認。Esc で中止。",
            "Terminal zum Bestätigen vergrößern. Esc bricht ab.",
            "Amplía la terminal para confirmar. Esc cancela.",
        ],
    ),
];
