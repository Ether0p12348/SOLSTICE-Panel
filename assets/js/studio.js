    const STUDIO_BOOTSTRAP = window.STUDIO_BOOTSTRAP && typeof window.STUDIO_BOOTSTRAP === 'object'
        ? window.STUDIO_BOOTSTRAP
        : {};
    const INITIAL_PAGE_CATALOG = STUDIO_BOOTSTRAP.pageCatalog;
    const INITIAL_PUBLISHED_SPEC = STUDIO_BOOTSTRAP.publishedSpec;

    let pageCatalog = Array.isArray(INITIAL_PAGE_CATALOG) ? INITIAL_PAGE_CATALOG : [];
    let catalogByKey = new Map();
    let currentPageKey = String(STUDIO_BOOTSTRAP.initialCatalogKey || '');
    let currentPageId = null;
    let currentPageEntry = null;
    let currentPageDefinition = null;
    let selectedElementIndex = null;

    let studioEventSource = null;
    let dragState = null;
    let elementAutoSaveTimer = null;

    let currentRuntimeSpec = {
        boot_sequence: [],
        rotation_interval_ms: 5000,
        rotation_queue: [],
    };

    const LEGACY_ID_ALIASES = {
        boot: 'solstice-panel-core-1.0.0-boot',
        live_info: 'solstice-panel-core-1.0.0-live-info',
        diagnostics: 'solstice-panel-core-1.0.0-diagnostics',
    };

    const LEGACY_DYNAMIC_SOURCE_ALIASES = {
        hostname: 'sys:hostname',
        ip_addr: 'sys:ip_addr',
        local_time_hm: 'sys:local_time_hm',
        local_time_hms: 'sys:local_time_hms',
        local_date_ymd: 'sys:local_date_ymd',
        local_date_dmy: 'sys:local_date_dmy',
        local_date_mdy: 'sys:local_date_mdy',
        local_datetime_iso: 'sys:local_datetime_iso',
        local_datetime_compact: 'sys:local_datetime_compact',
        local_datetime_rfc2822: 'sys:local_datetime_rfc2822',
        local_datetime_rfc3339: 'sys:local_datetime_rfc3339',
        uptime_text: 'sys:uptime_text',
        ram_percent_text: 'sys:ram_percent_text',
        cpu_temp_text: 'sys:cpu_temp_text',
        cpu_usage_percent: 'sys:cpu_usage_percent',
        load_avg_1: 'sys:load_avg_1',
        load_avg_5: 'sys:load_avg_5',
        load_avg_15: 'sys:load_avg_15',
        load_avg_text: 'sys:load_avg_text',
        mem_total_mib: 'sys:mem_total_mib',
        mem_used_mib: 'sys:mem_used_mib',
        mem_available_mib: 'sys:mem_available_mib',
        mem_free_mib: 'sys:mem_free_mib',
        swap_total_mib: 'sys:swap_total_mib',
        swap_used_mib: 'sys:swap_used_mib',
        swap_free_mib: 'sys:swap_free_mib',
        swap_used_percent_text: 'sys:swap_used_percent_text',
        procs_running: 'sys:procs_running',
        procs_blocked: 'sys:procs_blocked',
        cpu_cores: 'sys:cpu_cores',
        os_pretty_name: 'sys:os_pretty_name',
        kernel_release: 'sys:kernel_release',
        active_page: 'display:active_page',
        active_page_id: 'display:active_page_id',
        display_mode: 'display:mode',
        rotation_active: 'display:rotation_active',
        rotation_interval_ms: 'display:rotation_interval_ms',
        rotation_interval_seconds: 'display:rotation_interval_seconds',
        rotation_queue_len: 'display:rotation_queue_len',
        rotation_queue_empty: 'display:rotation_queue_empty',
        rotation_index: 'display:rotation_index',
        rotation_next_index: 'display:rotation_next_index',
        rotation_position: 'display:rotation_position',
        display_width: 'display:width',
        display_height: 'display:height',
        refresh_ms: 'config:refresh_ms',
        i2c_address: 'config:i2c_address',
        i2c_address_hex: 'config:i2c_address_hex',
        web_bind: 'config:web_bind',
        page_id: 'display:page_id',
        page_name: 'display:page_name',
        page_version: 'display:page_version',
        page_authors: 'display:page_authors',
        page_author_count: 'display:page_author_count',
        page_bundle: 'display:page_bundle',
        page_license: 'display:page_license',
        page_source_url: 'display:page_source_url',
        page_tags: 'display:page_tags',
        page_tag_count: 'display:page_tag_count',
        page_description: 'display:page_description',
        page_element_count: 'display:page_element_count',
        page_width: 'display:page_width',
        page_height: 'display:page_height',
        unix_epoch_seconds: 'sys:unix_epoch_seconds',
        unix_epoch_millis: 'sys:unix_epoch_millis',
    };

    const DYNAMIC_SOURCE_GROUPS = [
        {
            label: 'System',
            values: [
                'sys:hostname',
                'sys:ip_addr',
                'sys:local_time_hm',
                'sys:local_time_hms',
                'sys:local_date_ymd',
                'sys:local_date_dmy',
                'sys:local_date_mdy',
                'sys:local_datetime_iso',
                'sys:local_datetime_compact',
                'sys:local_datetime_rfc2822',
                'sys:local_datetime_rfc3339',
                'sys:uptime_text',
                'sys:ram_percent_text',
                'sys:cpu_temp_text',
                'sys:cpu_usage_percent',
                'sys:load_avg_1',
                'sys:load_avg_5',
                'sys:load_avg_15',
                'sys:load_avg_text',
                'sys:mem_total_mib',
                'sys:mem_used_mib',
                'sys:mem_available_mib',
                'sys:mem_free_mib',
                'sys:swap_total_mib',
                'sys:swap_used_mib',
                'sys:swap_free_mib',
                'sys:swap_used_percent_text',
                'sys:procs_running',
                'sys:procs_blocked',
                'sys:cpu_cores',
                'sys:os_pretty_name',
                'sys:kernel_release',
                'sys:unix_epoch_seconds',
                'sys:unix_epoch_millis',
            ],
        },
        {
            label: 'Display / Runtime',
            values: [
                'display:active_page',
                'display:active_page_id',
                'display:mode',
                'display:rotation_active',
                'display:rotation_interval_ms',
                'display:rotation_interval_seconds',
                'display:rotation_queue_len',
                'display:rotation_queue_empty',
                'display:rotation_index',
                'display:rotation_next_index',
                'display:rotation_position',
                'display:width',
                'display:height',
            ],
        },
        {
            label: 'Display / Current Page',
            values: [
                'display:page_id',
                'display:page_name',
                'display:page_version',
                'display:page_authors',
                'display:page_author_count',
                'display:page_bundle',
                'display:page_license',
                'display:page_source_url',
                'display:page_tags',
                'display:page_tag_count',
                'display:page_description',
                'display:page_element_count',
                'display:page_width',
                'display:page_height',
            ],
        },
        {
            label: 'Config',
            values: [
                'config:refresh_ms',
                'config:i2c_address',
                'config:i2c_address_hex',
                'config:web_bind',
            ],
        },
    ];

    function escapeHtml(value) {
        return String(value)
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    function clamp(value, min, max) {
        return Math.min(max, Math.max(min, value));
    }

    function parseIntSafe(value, fallback = 0) {
        const parsed = Number.parseInt(value, 10);
        return Number.isFinite(parsed) ? parsed : fallback;
    }

    function deepClone(value) {
        return JSON.parse(JSON.stringify(value));
    }

    function normalizeCatalogKey(key) {
        if (typeof key !== 'string') return '';
        let out = key;
        if (out.startsWith('custom:')) out = out.slice('custom:'.length);
        if (out.startsWith('system:')) out = out.slice('system:'.length);
        return LEGACY_ID_ALIASES[out] || out;
    }

    function indexCatalog() {
        catalogByKey = new Map(pageCatalog.map((entry) => [entry.key, entry]));
    }

    function defaultCatalogKey() {
        return pageCatalog[0]?.key || '';
    }

    function defaultBootKey() {
        return pageCatalog.find((p) => p.key.endsWith('-boot'))?.key || defaultCatalogKey();
    }

    function defaultRotationKey() {
        return pageCatalog.find((p) => p.key.includes('live-info'))?.key || defaultCatalogKey();
    }

    function pageIdFromSpecRef(value) {
        if (typeof value === 'string') return normalizeCatalogKey(value);
        if (value && typeof value === 'object' && typeof value.value === 'string') {
            return normalizeCatalogKey(value.value);
        }
        return '';
    }

    function currentPageIsEditable() {
        return Boolean(currentPageEntry?.editable && currentPageDefinition && currentPageId);
    }

    function ensurePageMeta(definition) {
        if (!definition.meta || typeof definition.meta !== 'object') {
            definition.meta = {};
        }
        definition.meta.schema_version = Number(definition.meta.schema_version) > 0
            ? Number(definition.meta.schema_version)
            : 1;
        if (!definition.meta.version || String(definition.meta.version).trim() === '') {
            definition.meta.version = '1.0.0';
        }
        if (!Array.isArray(definition.meta.authors)) {
            definition.meta.authors = [];
        }
        if (!Array.isArray(definition.meta.tags)) {
            definition.meta.tags = [];
        }
        if (typeof definition.meta.description !== 'string') {
            definition.meta.description = definition.meta.description == null
                ? null
                : String(definition.meta.description);
        }
        if (typeof definition.meta.bundle_name !== 'string') {
            definition.meta.bundle_name = definition.meta.bundle_name == null
                ? null
                : String(definition.meta.bundle_name);
        }
        if (typeof definition.meta.license !== 'string') {
            definition.meta.license = definition.meta.license == null
                ? null
                : String(definition.meta.license);
        }
        if (typeof definition.meta.source_url !== 'string') {
            definition.meta.source_url = definition.meta.source_url == null
                ? null
                : String(definition.meta.source_url);
        }
    }

    function getPageWidth() {
        return Number(currentPageDefinition?.width) || Number(document.getElementById('studioPreview').width) || 128;
    }

    function getPageHeight() {
        return Number(currentPageDefinition?.height) || Number(document.getElementById('studioPreview').height) || 32;
    }

    function baseTextMetricsForHeight(textHeightPx) {
        if (Number(textHeightPx) >= 8) {
            return {charWidth: 6, charHeight: 10};
        }
        return {charWidth: 4, charHeight: 6};
    }

    function normalizeTextHeightPx(raw, fallback = 6) {
        const parsed = Number.parseInt(String(raw ?? ''), 10);
        if (!Number.isFinite(parsed)) {
            return Math.max(5, fallback);
        }
        return Math.max(5, parsed);
    }

    function textMetricsForHeight(textHeightPx) {
        const normalizedHeight = normalizeTextHeightPx(textHeightPx, 6);
        const base = baseTextMetricsForHeight(normalizedHeight);
        const scaledWidth = Math.max(1, Math.round((base.charWidth * normalizedHeight) / base.charHeight));
        return {charWidth: scaledWidth, charHeight: normalizedHeight};
    }

    function elementLabel(element) {
        if (!element || typeof element !== 'object') return 'Element';
        const type = String(element.type || 'element');
        if (type === 'static_text') return 'Static Text';
        if (type === 'dynamic_text') return 'Dynamic Text';
        if (type === 'image') return 'Image';
        if (type === 'rect') return 'Rectangle';
        if (type === 'line') return 'Line';
        return type;
    }

    function elementDisplayName(element) {
        if (!element || typeof element !== 'object') return 'Element';
        const customName = typeof element.name === 'string' ? element.name.trim() : '';
        if (customName.length > 0) return customName;
        return elementLabel(element);
    }

    function formatMetaValue(value, fallback = '-') {
        if (Array.isArray(value)) {
            const cleaned = value
                .map((item) => String(item || '').trim())
                .filter((item) => item.length > 0);
            return cleaned.length ? cleaned.join(', ') : fallback;
        }
        if (value == null) return fallback;
        const text = String(value).trim();
        return text.length ? text : fallback;
    }

    function optionalTrimmedString(raw) {
        const value = String(raw ?? '').trim();
        return value.length ? value : null;
    }

    function canonicalDynamicSource(raw) {
        if (typeof raw !== 'string') {
            return 'sys:hostname';
        }
        const trimmed = raw.trim();
        if (!trimmed.length) {
            return 'sys:hostname';
        }
        return LEGACY_DYNAMIC_SOURCE_ALIASES[trimmed] || trimmed;
    }

    function renderDynamicSourceOptions(selectedSource) {
        const selectedCanonical = canonicalDynamicSource(selectedSource);
        return DYNAMIC_SOURCE_GROUPS.map((group) => {
            const options = group.values.map((source) => {
                const selected = source === selectedCanonical ? 'selected' : '';
                return `<option value="${escapeHtml(source)}" ${selected}>${escapeHtml(source)}</option>`;
            }).join('');
            return `<optgroup label="${escapeHtml(group.label)}">${options}</optgroup>`;
        }).join('');
    }

    function monoColorValue(raw, fallback = 'on') {
        return raw === 'off' ? 'off' : fallback;
    }

    function nullableMonoColorValue(raw) {
        if (raw === 'on' || raw === 'off') return raw;
        return null;
    }

    function textHeightPx(size) {
        return size === 'large' ? 10 : 6;
    }

    function sizeFromTextHeightPx(raw) {
        const px = normalizeTextHeightPx(raw, 6);
        return px >= 8 ? 'large' : 'small';
    }

    function elementTextHeightPx(element) {
        if (!element || typeof element !== 'object') return 6;
        const fallback = textHeightPx(element.size);
        return normalizeTextHeightPx(element.text_height_px, fallback);
    }

    function elementSummary(element) {
        if (!element || typeof element !== 'object') return 'Unknown element';
        switch (element.type) {
            case 'static_text':
                return `(${element.x}, ${element.y}) text`;
            case 'dynamic_text':
                return `(${element.x}, ${element.y}) ${canonicalDynamicSource(element.source || 'sys:hostname')}`;
            case 'image':
                return `(${element.x}, ${element.y}) ${element.w}x${element.h} ${String(element.source || '').trim() || '(no source)'}`;
            case 'rect':
                return `(${element.x}, ${element.y}) ${element.w}x${element.h}`;
            case 'line':
                return `(${element.x1}, ${element.y1}) -> (${element.x2}, ${element.y2})`;
            default:
                return 'Unsupported';
        }
    }

    function getElementBounds(element) {
        if (!element || typeof element !== 'object') {
            return {x: 0, y: 0, w: 1, h: 1};
        }

        switch (element.type) {
            case 'static_text': {
                const metrics = textMetricsForHeight(elementTextHeightPx(element));
                const text = String(element.text || '');
                const length = Math.max(1, text.length);
                const baselineY = Number(element.y) || 0;
                return {
                    x: Number(element.x) || 0,
                    // Text elements are drawn using baseline Y in embedded-graphics.
                    y: baselineY - (metrics.charHeight - 2),
                    w: Math.max(1, length * metrics.charWidth),
                    h: metrics.charHeight,
                };
            }
            case 'dynamic_text': {
                const metrics = textMetricsForHeight(elementTextHeightPx(element));
                const prefix = String(element.prefix || '');
                const maxChars = Number(element.max_chars) > 0 ? Number(element.max_chars) : 12;
                const length = Math.max(1, prefix.length + maxChars);
                const baselineY = Number(element.y) || 0;
                return {
                    x: Number(element.x) || 0,
                    // Text elements are drawn using baseline Y in embedded-graphics.
                    y: baselineY - (metrics.charHeight - 2),
                    w: Math.max(1, length * metrics.charWidth),
                    h: metrics.charHeight,
                };
            }
            case 'image':
                return {
                    x: Number(element.x) || 0,
                    y: Number(element.y) || 0,
                    w: Math.max(1, Number(element.w) || 1),
                    h: Math.max(1, Number(element.h) || 1),
                };
            case 'rect':
                return {
                    x: Number(element.x) || 0,
                    y: Number(element.y) || 0,
                    w: Math.max(1, Number(element.w) || 1),
                    h: Math.max(1, Number(element.h) || 1),
                };
            case 'line': {
                const x1 = Number(element.x1) || 0;
                const y1 = Number(element.y1) || 0;
                const x2 = Number(element.x2) || 0;
                const y2 = Number(element.y2) || 0;
                return {
                    x: Math.min(x1, x2),
                    y: Math.min(y1, y2),
                    w: Math.max(1, Math.abs(x2 - x1) + 1),
                    h: Math.max(1, Math.abs(y2 - y1) + 1),
                };
            }
            default:
                return {x: 0, y: 0, w: 1, h: 1};
        }
    }

    function clampElementPosition(element) {
        const pageW = getPageWidth();
        const pageH = getPageHeight();

        if (!element || typeof element !== 'object') return element;

        switch (element.type) {
            case 'static_text':
            case 'dynamic_text': {
                element.x = clamp(parseIntSafe(element.x), 0, Math.max(0, pageW - 1));
                element.y = clamp(parseIntSafe(element.y), 0, Math.max(0, pageH - 1));
                break;
            }
            case 'rect': {
                element.w = Math.max(1, parseIntSafe(element.w, 1));
                element.h = Math.max(1, parseIntSafe(element.h, 1));
                element.x = clamp(parseIntSafe(element.x), 0, Math.max(0, pageW - element.w));
                element.y = clamp(parseIntSafe(element.y), 0, Math.max(0, pageH - element.h));
                element.filled = Boolean(element.filled);
                break;
            }
            case 'image': {
                element.w = Math.max(1, parseIntSafe(element.w, 1));
                element.h = Math.max(1, parseIntSafe(element.h, 1));
                element.x = clamp(parseIntSafe(element.x), 0, Math.max(0, pageW - element.w));
                element.y = clamp(parseIntSafe(element.y), 0, Math.max(0, pageH - element.h));
                break;
            }
            case 'line': {
                const x1 = parseIntSafe(element.x1);
                const x2 = parseIntSafe(element.x2);
                const y1 = parseIntSafe(element.y1);
                const y2 = parseIntSafe(element.y2);
                element.x1 = clamp(x1, 0, Math.max(0, pageW - 1));
                element.y1 = clamp(y1, 0, Math.max(0, pageH - 1));
                element.x2 = clamp(x2, 0, Math.max(0, pageW - 1));
                element.y2 = clamp(y2, 0, Math.max(0, pageH - 1));
                break;
            }
            default:
                break;
        }

        return element;
    }

    function getCanvasScale() {
        const canvas = document.getElementById('studioPreview');
        const rect = canvas.getBoundingClientRect();

        return {
            x: rect.width / canvas.width,
            y: rect.height / canvas.height,
        };
    }

    function oledCoordFromPointer(event) {
        const canvas = document.getElementById('studioPreview');
        const rect = canvas.getBoundingClientRect();

        const rawX = ((event.clientX - rect.left) * canvas.width) / rect.width;
        const rawY = ((event.clientY - rect.top) * canvas.height) / rect.height;

        return {
            x: clamp(Math.floor(rawX), 0, canvas.width - 1),
            y: clamp(Math.floor(rawY), 0, canvas.height - 1),
        };
    }

    function updateCanvasHint(message = '') {
        const hint = document.getElementById('canvasHint');
        hint.textContent = message;
    }

    function updateCanvasStatus() {
        const pageMetaMain = document.getElementById('canvasPageMetaMain');
        const pageMetaList = document.getElementById('canvasPageMetaList');
        const selectionMeta = document.getElementById('canvasSelectionMeta');
        const emptyState = document.getElementById('canvasEmptyState');

        if (!pageMetaMain || !pageMetaList || !selectionMeta || !emptyState) return;

        if (!currentPageDefinition) {
            pageMetaMain.textContent = 'No page selected';
            pageMetaList.innerHTML = '';
            selectionMeta.textContent = 'None';
            emptyState.classList.add('visible');
            return;
        }

        ensurePageMeta(currentPageDefinition);

        const pageName = currentPageDefinition.name || currentPageId || currentPageKey || 'Unnamed';
        const pageId = currentPageDefinition.id || currentPageId || currentPageKey || 'unknown';
        const elementCount = Array.isArray(currentPageDefinition.elements) ? currentPageDefinition.elements.length : 0;
        const meta = currentPageDefinition.meta || {};
        const metaTags = [
            ['version', formatMetaValue(meta.version, '1.0.0')],
            ['authors', formatMetaValue(meta.authors)],
            ['bundle', formatMetaValue(meta.bundle_name)],
            ['license', formatMetaValue(meta.license)],
            ['source', formatMetaValue(meta.source_url)],
            ['tags', formatMetaValue(meta.tags)],
            ['description', formatMetaValue(meta.description)],
        ];

        pageMetaMain.textContent = `${pageName} (${pageId}) · ${getPageWidth()}x${getPageHeight()} · ${elementCount} elements`;
        pageMetaList.innerHTML = metaTags.map(([label, value]) => (
            `<span class="canvas-page-meta-tag"><b>${escapeHtml(label)}</b> ${escapeHtml(value)}</span>`
        )).join('');
        emptyState.classList.remove('visible');

        if (selectedElementIndex == null || !currentPageDefinition.elements[selectedElementIndex]) {
            selectionMeta.textContent = 'Page';
            return;
        }

        const element = currentPageDefinition.elements[selectedElementIndex];
        const bounds = getElementBounds(element);
        selectionMeta.textContent = `${elementDisplayName(element)} #${selectedElementIndex + 1} @ ${bounds.x},${bounds.y}`;
    }

    function updateModeBadge() {
        const badge = document.getElementById('pageEditorModeBadge');
        const selectionSummary = document.getElementById('inspectorSelectionSummary');
        if (!currentPageDefinition) {
            badge.textContent = 'No Page';
            if (selectionSummary) {
                selectionSummary.textContent = 'Select a page to view properties.';
            }
            updateCanvasStatus();
            return;
        }

        if (selectedElementIndex == null) {
            badge.textContent = currentPageIsEditable() ? 'Page Selection' : 'Read-Only Page';
            if (selectionSummary) {
                selectionSummary.textContent = 'Page properties appear when no element is selected.';
            }
            updateCanvasStatus();
            return;
        }

        const element = currentPageDefinition.elements[selectedElementIndex];
        if (!element) {
            badge.textContent = 'Page Selection';
            if (selectionSummary) {
                selectionSummary.textContent = 'Page properties appear when no element is selected.';
            }
            updateCanvasStatus();
            return;
        }

        badge.textContent = `${elementDisplayName(element)} · #${selectedElementIndex + 1}`;
        if (selectionSummary) {
            selectionSummary.textContent = `${elementDisplayName(element)} selected. Edit properties in the inspector.`;
        }
        updateCanvasStatus();
    }

    function setInspectorFeedback(message, ok = false) {
        const el = document.getElementById('inspectorFeedback');
        el.textContent = message || '';
        el.className = 'inspector-feedback';
        if (message) {
            el.classList.add(ok ? 'ok' : 'error');
        }
    }

    function setEditableState(editable) {
        const editorBlock = document.getElementById('pageEditorBlock');
        editorBlock.classList.toggle('disabled-block', !editable);

        document.querySelectorAll('.requires-editable').forEach((el) => {
            el.disabled = !editable;
        });

        document.getElementById('editorModeHelp').textContent = editable
            ? 'Use the canvas or object list to select and edit.'
            : 'The selected page is not editable.';
    }

    function normalizeRuntimeSpec(rawSpec) {
        const spec = rawSpec && typeof rawSpec === 'object' ? rawSpec : {};

        const boot_sequence = Array.isArray(spec.boot_sequence)
            ? spec.boot_sequence.map((step) => ({
                page_key: pageIdFromSpecRef(step.page_id ?? step.page_ref),
                duration_ms: Number(step.duration_ms) || 2000,
            }))
            : [];

        const rotation_queue = Array.isArray(spec.rotation_queue)
            ? spec.rotation_queue.map((pageId) => pageIdFromSpecRef(pageId))
            : [];

        return {
            boot_sequence,
            rotation_interval_ms: Number(spec.rotation_interval_ms) || 5000,
            rotation_queue,
        };
    }

    function runtimeSpecToApiPayload() {
        return {
            boot_sequence: currentRuntimeSpec.boot_sequence.map((step) => ({
                page_id: normalizeCatalogKey(step.page_key),
                duration_ms: Number(step.duration_ms),
            })),
            rotation_interval_ms: Number(currentRuntimeSpec.rotation_interval_ms),
            rotation_queue: currentRuntimeSpec.rotation_queue.map((key) => normalizeCatalogKey(key)),
        };
    }

    function validateRuntimeSpec() {
        const errors = [];

        if (!Number.isFinite(Number(currentRuntimeSpec.rotation_interval_ms)) || Number(currentRuntimeSpec.rotation_interval_ms) < 250) {
            errors.push('Rotation interval must be at least 250 ms.');
        }

        if (!Array.isArray(currentRuntimeSpec.rotation_queue) || currentRuntimeSpec.rotation_queue.length === 0) {
            errors.push('Rotation Queue must have at least one entry.');
        }

        currentRuntimeSpec.boot_sequence.forEach((step, idx) => {
            if (!Number.isFinite(Number(step.duration_ms)) || Number(step.duration_ms) < 100) {
                errors.push(`Boot Sequence step ${idx + 1} duration must be at least 100 ms.`);
            }
            if (!catalogByKey.has(step.page_key)) {
                errors.push(`Boot Sequence step ${idx + 1} references a page missing from the catalog.`);
            }
        });

        currentRuntimeSpec.rotation_queue.forEach((key, idx) => {
            if (!catalogByKey.has(key)) {
                errors.push(`Rotation Queue entry ${idx + 1} references a page missing from the catalog.`);
            }
        });

        return errors;
    }

    async function getJson(url) {
        const res = await fetch(url);
        const data = await res.json();
        if (!res.ok) {
            throw new Error(data.error || 'Request failed');
        }
        return data;
    }

    async function getText(url) {
        const res = await fetch(url);
        const data = await res.text();
        if (!res.ok) {
            let message = data;
            try {
                const parsed = JSON.parse(data);
                message = parsed.error || data;
            } catch (_) {}
            throw new Error(message || 'Request failed');
        }
        return data;
    }

    async function postJson(url, body = null, showAlert = true) {
        const options = {method: 'POST', headers: {}};

        if (body !== null) {
            options.headers['Content-Type'] = 'application/json';
            options.body = JSON.stringify(body);
        }

        const res = await fetch(url, options);
        const data = await res.json();
        if (!res.ok) {
            if (showAlert) {
                alert(data.error || 'Request failed');
            }
            throw new Error(data.error || 'Request failed');
        }
        return data;
    }

    function openTransferDialog() {
        const dialog = document.getElementById('transferDialog');
        if (!dialog) return;
        if (!dialog.open) {
            dialog.showModal();
        }
    }

    function closeTransferDialog() {
        const dialog = document.getElementById('transferDialog');
        if (!dialog) return;
        dialog.close();
    }

    function setPageTransferFeedback(message, ok = false) {
        const el = document.getElementById('pageTransferFeedback');
        el.textContent = message;
        el.className = ok ? 'runtime-feedback ok' : 'runtime-feedback error';
        if (!message) {
            el.className = 'runtime-feedback muted';
        }
    }

    function setRuntimeFeedback(message, ok = false) {
        const el = document.getElementById('publishedRuntimeFeedback');
        el.textContent = message;
        el.className = ok ? 'runtime-feedback ok' : 'runtime-feedback error';
        if (!message) {
            el.className = 'runtime-feedback muted';
        }
    }

    function drawPreviewFrame(frame) {
        const canvas = document.getElementById('studioPreview');
        let resized = false;
        if (canvas.width !== frame.width) {
            canvas.width = frame.width;
            resized = true;
        }
        if (canvas.height !== frame.height) {
            canvas.height = frame.height;
            resized = true;
        }

        const ctx = canvas.getContext('2d');
        ctx.imageSmoothingEnabled = false;

        const imageData = ctx.createImageData(frame.width, frame.height);

        for (let i = 0; i < frame.pixels.length; i++) {
            const on = frame.pixels[i] === 1;
            const offset = i * 4;
            const value = on ? 255 : 0;
            imageData.data[offset] = value;
            imageData.data[offset + 1] = value;
            imageData.data[offset + 2] = value;
            imageData.data[offset + 3] = 255;
        }

        ctx.putImageData(imageData, 0, 0);
        if (resized) {
            renderCanvasOverlay();
        }
    }

    function startStudioPreviewStream(pageKey) {
        if (studioEventSource) {
            studioEventSource.close();
        }

        studioEventSource = new EventSource(`/api/studio/events/${encodeURIComponent(pageKey)}`);
        studioEventSource.addEventListener('draft_preview', (event) => {
            try {
                const snapshot = JSON.parse(event.data);
                drawPreviewFrame(snapshot.frame);
            } catch (err) {
                console.error('failed to parse draft preview SSE payload:', err);
            }
        });
        studioEventSource.onerror = (err) => {
            console.error('studio SSE error:', err);
        };
    }

    function renderCatalogList() {
        const pageList = document.getElementById('pageCatalogList');

        if (!pageCatalog.length) {
            pageList.innerHTML = '<div class="muted">No pages available.</div>';
            return;
        }

        pageList.innerHTML = pageCatalog.map((page) => {
            const selected = page.key === currentPageKey ? 'active' : '';
            const detail = `${page.page_id || page.key} · ${page.element_count || 0} elements`;
            return `
        <button class="${selected}" onclick="loadCatalogPage(decodeURIComponent('${encodeURIComponent(page.key)}'))">
          <strong>${escapeHtml(page.display_name)}</strong><br>
          <span class="muted">${escapeHtml(detail)}</span>
        </button>
      `;
        }).join('');
    }

    function renderCanvasOverlay() {
        const overlay = document.getElementById('studioOverlay');

        if (!currentPageDefinition || !Array.isArray(currentPageDefinition.elements)) {
            overlay.innerHTML = '';
            updateCanvasStatus();
            return;
        }

        const scale = getCanvasScale();
        overlay.innerHTML = currentPageDefinition.elements.map((element, index) => {
            const bounds = getElementBounds(element);
            const left = Math.round(bounds.x * scale.x);
            const top = Math.round(bounds.y * scale.y);
            const width = Math.max(4, Math.round(bounds.w * scale.x));
            const height = Math.max(4, Math.round(bounds.h * scale.y));
            const selected = index === selectedElementIndex ? 'selected' : '';

            return `<button
                type="button"
                class="overlay-hit ${selected}"
                data-element-type="${escapeHtml(element.type || 'element')}"
                data-layer="${index + 1}"
                aria-label="${escapeHtml(elementLabel(element))} layer ${index + 1}"
                aria-pressed="${selected ? 'true' : 'false'}"
                style="left:${left}px; top:${top}px; width:${width}px; height:${height}px;"
                title="${escapeHtml(elementDisplayName(element))} #${index + 1}"
                onpointerdown="beginElementDrag(event, ${index})"
                onclick="selectElement(${index}); event.stopPropagation();"
            ></button>`;
        }).join('');

        overlay.onclick = (event) => {
            if (event.target === overlay) {
                selectElement(null);
            }
        };

        updateCanvasStatus();
    }

    function renderElements() {
        const elementsList = document.getElementById('elementsList');
        const countBadge = document.getElementById('layerCountBadge');

        if (!currentPageDefinition || !Array.isArray(currentPageDefinition.elements)) {
            elementsList.innerHTML = '<div class="muted">No page selected.</div>';
            countBadge.textContent = '0';
            return;
        }

        const elements = currentPageDefinition.elements;
        countBadge.textContent = String(elements.length);

        if (elements.length === 0) {
            elementsList.innerHTML = '<div class="muted">No elements yet. Use Insert tools to add your first element.</div>';
            return;
        }

        elementsList.innerHTML = elements.map((element, index) => {
            const selected = index === selectedElementIndex ? 'selected' : '';
            const shortType = (() => {
                switch (element.type) {
                    case 'static_text': return 'TXT';
                    case 'dynamic_text': return 'DYN';
                    case 'image': return 'IMG';
                    case 'rect': return 'RECT';
                    case 'line': return 'LINE';
                    default: return 'ELM';
                }
            })();
            const orderLabel = index === elements.length - 1 ? 'Top layer' : (index === 0 ? 'Base layer' : `Layer ${index + 1}`);
            return `
        <div class="layer-row ${selected}" data-element-type="${escapeHtml(element.type || 'element')}" onclick="selectElement(${index})">
          <div class="layer-main">
            <span class="layer-kind">${shortType}</span>
            <div class="layer-text">
              <div class="layer-head">
                <strong>${escapeHtml(elementDisplayName(element))}</strong>
                <span class="layer-order">${orderLabel}</span>
              </div>
              <div class="layer-meta">${escapeHtml(elementSummary(element))}</div>
            </div>
            <span class="layer-index">#${index + 1}</span>
          </div>
          <div class="layer-controls">
            <button class="mini-btn ghost requires-editable" onclick="event.stopPropagation(); moveElementInList(${index}, -1)">Move Up</button>
            <button class="mini-btn ghost requires-editable" onclick="event.stopPropagation(); moveElementInList(${index}, 1)">Move Down</button>
            <button class="mini-btn danger requires-editable" onclick="event.stopPropagation(); deleteElement(${index})">Delete</button>
          </div>
        </div>
      `;
        }).join('');
    }

    function renderInspector() {
        const panel = document.getElementById('inspectorPanel');
        const scope = document.getElementById('inspectorScopeBadge');

        if (!currentPageDefinition) {
            scope.textContent = 'None';
            panel.innerHTML = '<div class="muted">Select a page to view inspector properties.</div>';
            updateModeBadge();
            return;
        }

        ensurePageMeta(currentPageDefinition);

        if (selectedElementIndex == null || !currentPageDefinition.elements[selectedElementIndex]) {
            scope.textContent = 'Page';
            renderPageInspector(panel);
        } else {
            scope.textContent = 'Element';
            renderElementInspector(panel, currentPageDefinition.elements[selectedElementIndex]);
        }

        updateModeBadge();
    }

    function renderPageInspector(panel) {
        const definition = currentPageDefinition;
        const authors = Array.isArray(definition.meta.authors) ? definition.meta.authors.join(', ') : '';
        const tags = Array.isArray(definition.meta.tags) ? definition.meta.tags.join(', ') : '';
        const schemaVersion = Number(definition.meta.schema_version) || 1;
        const description = formatMetaValue(definition.meta.description, '');
        const bundleName = formatMetaValue(definition.meta.bundle_name, '');
        const license = formatMetaValue(definition.meta.license, '');
        const sourceUrl = formatMetaValue(definition.meta.source_url, '');

        panel.innerHTML = `
      <div class="inspector-group">
        <p class="inspector-kicker">Page</p>
        <h3>Page Properties</h3>
        <p class="inspector-note">Update page identity and publishing metadata when nothing is selected.</p>
        <label>
          Page ID
          <input id="pageIdInput" class="requires-editable" type="text" value="${escapeHtml(definition.id)}">
        </label>
        <div class="inspector-grid">
          <label>
            Name
            <input id="pageNameInput" class="requires-editable" type="text" value="${escapeHtml(definition.name)}">
          </label>
          <label>
            Version
            <input id="pageVersionInput" class="requires-editable" type="text" value="${escapeHtml(definition.meta.version || '1.0.0')}">
          </label>
        </div>
        <label>
          Authors (comma-separated)
          <input id="pageAuthorsInput" class="requires-editable" type="text" value="${escapeHtml(authors)}">
        </label>
        <div class="inspector-grid">
          <label>
            Bundle Name
            <input id="pageBundleNameInput" class="requires-editable" type="text" value="${escapeHtml(bundleName)}" placeholder="core">
          </label>
          <label>
            License
            <input id="pageLicenseInput" class="requires-editable" type="text" value="${escapeHtml(license)}" placeholder="MIT">
          </label>
        </div>
        <label>
          Source URL
          <input id="pageSourceUrlInput" class="requires-editable" type="text" value="${escapeHtml(sourceUrl)}" placeholder="https://...">
        </label>
        <label>
          Tags (comma-separated)
          <input id="pageTagsInput" class="requires-editable" type="text" value="${escapeHtml(tags)}" placeholder="runtime, status, diagnostics">
        </label>
        <label>
          Description
          <textarea id="pageDescriptionInput" class="requires-editable" placeholder="What this page is for...">${escapeHtml(description)}</textarea>
        </label>
        <div class="muted">Schema version: <strong>v${escapeHtml(schemaVersion)}</strong> (auto-managed)</div>
        <div class="inspector-actions">
          <button class="primary requires-editable" onclick="applyPageMetadataFromInspector()">Apply Metadata</button>
          <button class="ghost" onclick="applyCurrentPage()">Apply to OLED</button>
          <button class="ghost danger requires-editable" onclick="deleteCurrentPage()">Delete Page</button>
        </div>
      </div>

      <details class="advanced">
        <summary>Advanced: Raw Page JSON</summary>
        <div class="stack" style="margin-top: 0.7rem;">
          <textarea id="pageJsonEditor" class="requires-editable">${escapeHtml(JSON.stringify(definition, null, 2))}</textarea>
          <div class="row">
            <button class="requires-editable" onclick="savePageJson()">Save Raw Page JSON</button>
          </div>
        </div>
      </details>
    `;

        setEditableState(currentPageIsEditable());
    }

    function renderElementInspector(panel, element) {
        const index = selectedElementIndex;
        const elementName = typeof element.name === 'string' ? element.name : '';
        const textColor = monoColorValue(element.color, 'on');
        let rectFill = nullableMonoColorValue(element.fill);
        let rectStroke = nullableMonoColorValue(element.stroke);
        if (element.type === 'rect' && rectFill === null && rectStroke === null) {
            if (element.filled) {
                rectFill = 'on';
            } else {
                rectStroke = 'on';
            }
        }
        const commonHeader = `
      <div class="inspector-group">
        <p class="inspector-kicker">Selection</p>
        <h3>${escapeHtml(elementDisplayName(element))} Properties</h3>
        <p class="inspector-selection-line">Layer #${index + 1} · ${escapeHtml(elementLabel(element))} · ${escapeHtml(elementSummary(element))}</p>
    `;
        const footer = `
        <div class="inspector-actions">
          <button class="ghost danger requires-editable" onclick="deleteElement(${index})">Delete Element</button>
        </div>
      </div>

      <details class="advanced">
        <summary>Advanced: Raw Selected Element JSON</summary>
        <div class="stack" style="margin-top: 0.7rem;">
          <textarea id="elementJsonEditor" class="requires-editable">${escapeHtml(JSON.stringify(element, null, 2))}</textarea>
          <div class="row">
            <button class="requires-editable" onclick="saveSelectedElementJson()">Save Element JSON</button>
          </div>
        </div>
      </details>
    `;

        let body = `
        <label>
          Element Name
          <input id="elementName" class="requires-editable" type="text" placeholder="${escapeHtml(elementLabel(element))}" value="${escapeHtml(elementName)}" oninput="onElementInspectorInputChanged()">
        </label>
      `;
        if (element.type === 'static_text') {
            const staticTextHeight = elementTextHeightPx(element);
            body += `
        <div class="inspector-grid">
          <label>X<input id="elementX" class="requires-editable" type="number" value="${escapeHtml(element.x)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y<input id="elementY" class="requires-editable" type="number" value="${escapeHtml(element.y)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <label>Text<input id="elementText" class="requires-editable" type="text" value="${escapeHtml(element.text || '')}" oninput="onElementInspectorInputChanged()"></label>
        <label>
          Text Height (px)
          <input id="elementSize" class="requires-editable" type="number" min="5" step="1" value="${escapeHtml(staticTextHeight)}" oninput="onElementInspectorInputChanged()">
        </label>
        <label>
          Color
          <select id="elementColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
            <option value="on" ${textColor === 'on' ? 'selected' : ''}>on (white)</option>
            <option value="off" ${textColor === 'off' ? 'selected' : ''}>off (black)</option>
          </select>
        </label>
      `;
        } else if (element.type === 'dynamic_text') {
            const source = canonicalDynamicSource(String(element.source || 'sys:hostname'));
            const dynamicTextHeight = elementTextHeightPx(element);
            body += `
        <div class="inspector-grid">
          <label>X<input id="elementX" class="requires-editable" type="number" value="${escapeHtml(element.x)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y<input id="elementY" class="requires-editable" type="number" value="${escapeHtml(element.y)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <label>
          Source
          <select id="elementSource" class="requires-editable" onchange="onElementInspectorInputChanged()">
            ${renderDynamicSourceOptions(source)}
          </select>
        </label>
        <div class="inspector-grid">
          <label>Prefix<input id="elementPrefix" class="requires-editable" type="text" value="${escapeHtml(element.prefix || '')}" oninput="onElementInspectorInputChanged()"></label>
          <label>Max chars<input id="elementMaxChars" class="requires-editable" type="number" min="0" value="${escapeHtml(element.max_chars ?? 20)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <label>
          Text Height (px)
          <input id="elementSize" class="requires-editable" type="number" min="5" step="1" value="${escapeHtml(dynamicTextHeight)}" oninput="onElementInspectorInputChanged()">
        </label>
        <label>
          Color
          <select id="elementColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
            <option value="on" ${textColor === 'on' ? 'selected' : ''}>on (white)</option>
            <option value="off" ${textColor === 'off' ? 'selected' : ''}>off (black)</option>
          </select>
        </label>
      `;
        } else if (element.type === 'image') {
            const imageMaskMode = String(element.mask_mode || 'alpha');
            const imageThreshold = Number.isFinite(Number(element.threshold))
                ? Math.max(0, Math.min(255, Number(element.threshold)))
                : 128;
            const imageForeground = monoColorValue(element.foreground, 'on');
            const imageBackground = nullableMonoColorValue(element.background);
            body += `
        <div class="inspector-grid">
          <label>X<input id="elementX" class="requires-editable" type="number" value="${escapeHtml(element.x)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y<input id="elementY" class="requires-editable" type="number" value="${escapeHtml(element.y)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Width<input id="elementW" class="requires-editable" type="number" min="1" value="${escapeHtml(element.w)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Height<input id="elementH" class="requires-editable" type="number" min="1" value="${escapeHtml(element.h)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <label>
          Source (path or data URI)
          <input id="elementSourcePath" class="requires-editable" type="text" value="${escapeHtml(String(element.source || ''))}" placeholder="assets/icons/logo.svg" oninput="onElementInspectorInputChanged()">
        </label>
        <div class="inspector-grid">
          <label>
            Mask Mode
            <select id="elementMaskMode" class="requires-editable" onchange="onElementInspectorInputChanged()">
              <option value="alpha" ${imageMaskMode === 'alpha' ? 'selected' : ''}>alpha (opaque shape)</option>
              <option value="alpha_inverted" ${imageMaskMode === 'alpha_inverted' ? 'selected' : ''}>alpha inverted</option>
              <option value="luma_light" ${imageMaskMode === 'luma_light' ? 'selected' : ''}>luma light (white shape)</option>
              <option value="luma_dark" ${imageMaskMode === 'luma_dark' ? 'selected' : ''}>luma dark (black shape)</option>
            </select>
          </label>
          <label>
            Threshold (0-255)
            <input id="elementThreshold" class="requires-editable" type="number" min="0" max="255" step="1" value="${escapeHtml(imageThreshold)}" oninput="onElementInspectorInputChanged()">
          </label>
        </div>
        <div class="inspector-grid">
          <label>
            Shape Color
            <select id="elementForegroundColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
              <option value="on" ${imageForeground === 'on' ? 'selected' : ''}>on (white)</option>
              <option value="off" ${imageForeground === 'off' ? 'selected' : ''}>off (black)</option>
            </select>
          </label>
          <label>
            Background
            <select id="elementBackgroundColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
              <option value="null" ${imageBackground === null ? 'selected' : ''}>transparent/no draw</option>
              <option value="on" ${imageBackground === 'on' ? 'selected' : ''}>on (white)</option>
              <option value="off" ${imageBackground === 'off' ? 'selected' : ''}>off (black)</option>
            </select>
          </label>
        </div>
      `;
        } else if (element.type === 'rect') {
            body += `
        <div class="inspector-grid">
          <label>X<input id="elementX" class="requires-editable" type="number" value="${escapeHtml(element.x)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y<input id="elementY" class="requires-editable" type="number" value="${escapeHtml(element.y)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Width<input id="elementW" class="requires-editable" type="number" min="1" value="${escapeHtml(element.w)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Height<input id="elementH" class="requires-editable" type="number" min="1" value="${escapeHtml(element.h)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <div class="inspector-grid">
          <label>
            Fill
            <select id="elementFillColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
              <option value="null" ${rectFill === null ? 'selected' : ''}>none</option>
              <option value="on" ${rectFill === 'on' ? 'selected' : ''}>on (white)</option>
              <option value="off" ${rectFill === 'off' ? 'selected' : ''}>off (black)</option>
            </select>
          </label>
          <label>
            Border
            <select id="elementStrokeColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
              <option value="null" ${rectStroke === null ? 'selected' : ''}>none</option>
              <option value="on" ${rectStroke === 'on' ? 'selected' : ''}>on (white)</option>
              <option value="off" ${rectStroke === 'off' ? 'selected' : ''}>off (black)</option>
            </select>
          </label>
        </div>
      `;
        } else if (element.type === 'line') {
            const lineColor = monoColorValue(element.color, 'on');
            body += `
        <div class="inspector-grid">
          <label>X1<input id="elementX1" class="requires-editable" type="number" value="${escapeHtml(element.x1)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y1<input id="elementY1" class="requires-editable" type="number" value="${escapeHtml(element.y1)}" oninput="onElementInspectorInputChanged()"></label>
          <label>X2<input id="elementX2" class="requires-editable" type="number" value="${escapeHtml(element.x2)}" oninput="onElementInspectorInputChanged()"></label>
          <label>Y2<input id="elementY2" class="requires-editable" type="number" value="${escapeHtml(element.y2)}" oninput="onElementInspectorInputChanged()"></label>
        </div>
        <label>
          Color
          <select id="elementColor" class="requires-editable" onchange="onElementInspectorInputChanged()">
            <option value="on" ${lineColor === 'on' ? 'selected' : ''}>on (white)</option>
            <option value="off" ${lineColor === 'off' ? 'selected' : ''}>off (black)</option>
          </select>
        </label>
      `;
        } else {
            body += '<div class="muted">Unsupported element type.</div>';
        }

        panel.innerHTML = `${commonHeader}${body}${footer}`;
        setEditableState(currentPageIsEditable());
    }

    function selectElement(index) {
        if (!currentPageDefinition || !Array.isArray(currentPageDefinition.elements)) {
            selectedElementIndex = null;
        } else if (index == null || !Number.isInteger(index) || index < 0 || index >= currentPageDefinition.elements.length) {
            selectedElementIndex = null;
        } else {
            selectedElementIndex = index;
        }

        renderCanvasOverlay();
        renderElements();
        renderInspector();

        if (selectedElementIndex == null) {
            updateCanvasHint('Page selected. Click an element on the canvas or in the object list.');
        } else {
            const element = currentPageDefinition.elements[selectedElementIndex];
            updateCanvasHint(`Selected ${elementDisplayName(element)} #${selectedElementIndex + 1}. Drag on canvas to move.`);
        }
    }

    async function persistCurrentPageDefinition(shouldRefreshCatalog = false) {
        if (!currentPageId || !currentPageDefinition) return;

        ensurePageMeta(currentPageDefinition);

        await postJson(`/api/studio/pages/${encodeURIComponent(currentPageId)}/replace`, currentPageDefinition, false);

        const existing = catalogByKey.get(currentPageKey);
        if (existing) {
            existing.display_name = currentPageDefinition.name;
            existing.element_count = Array.isArray(currentPageDefinition.elements) ? currentPageDefinition.elements.length : 0;
        }
        renderCatalogList();

        if (shouldRefreshCatalog) {
            await refreshCatalog();
        }
    }

    function updateCurrentPageDefinitionLocal() {
        if (!currentPageDefinition) return;
        ensurePageMeta(currentPageDefinition);
        const editor = document.getElementById('pageJsonEditor');
        if (editor && document.activeElement !== editor) {
            editor.value = JSON.stringify(currentPageDefinition, null, 2);
        }
    }

    async function saveCurrentPageQuick() {
        if (!currentPageDefinition || !currentPageIsEditable()) return;
        try {
            await persistCurrentPageDefinition(true);
            setInspectorFeedback('Page saved.', true);
            await loadCatalogPage(currentPageKey);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save page.', false);
        }
    }

    async function applyPageMetadataFromInspector() {
        if (!currentPageDefinition || !currentPageIsEditable()) return;

        const requestedPageId = document.getElementById('pageIdInput')?.value?.trim();
        const name = document.getElementById('pageNameInput')?.value?.trim();
        const version = document.getElementById('pageVersionInput')?.value?.trim();
        const authorsRaw = document.getElementById('pageAuthorsInput')?.value ?? '';
        const bundleNameRaw = document.getElementById('pageBundleNameInput')?.value ?? '';
        const licenseRaw = document.getElementById('pageLicenseInput')?.value ?? '';
        const sourceUrlRaw = document.getElementById('pageSourceUrlInput')?.value ?? '';
        const tagsRaw = document.getElementById('pageTagsInput')?.value ?? '';
        const descriptionRaw = document.getElementById('pageDescriptionInput')?.value ?? '';

        if (!requestedPageId) {
            setInspectorFeedback('Page ID cannot be empty.', false);
            return;
        }
        if (!name) {
            setInspectorFeedback('Page name cannot be empty.', false);
            return;
        }

        if (currentPageId && requestedPageId !== currentPageId) {
            try {
                const data = await postJson(
                    `/api/studio/pages/${encodeURIComponent(currentPageId)}/rekey`,
                    {new_id: requestedPageId},
                    false,
                );
                currentPageId = data.id || requestedPageId;
                currentPageKey = currentPageId;
                currentPageDefinition.id = currentPageId;
            } catch (err) {
                setInspectorFeedback(err.message || 'Failed to update page ID.', false);
                return;
            }
        } else {
            currentPageDefinition.id = requestedPageId;
        }

        currentPageDefinition.name = name;
        ensurePageMeta(currentPageDefinition);
        currentPageDefinition.meta.version = version || '1.0.0';
        currentPageDefinition.meta.authors = authorsRaw
            .split(',')
            .map((item) => item.trim())
            .filter((item) => item.length > 0);
        currentPageDefinition.meta.bundle_name = optionalTrimmedString(bundleNameRaw);
        currentPageDefinition.meta.license = optionalTrimmedString(licenseRaw);
        currentPageDefinition.meta.source_url = optionalTrimmedString(sourceUrlRaw);
        currentPageDefinition.meta.description = optionalTrimmedString(descriptionRaw);
        currentPageDefinition.meta.tags = tagsRaw
            .split(',')
            .map((item) => item.trim())
            .filter((item) => item.length > 0);

        updateCurrentPageDefinitionLocal();

        try {
            await persistCurrentPageDefinition(true);
            setInspectorFeedback('Page metadata saved.', true);
            await loadCatalogPage(currentPageKey);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save page metadata.', false);
        }
    }

    async function savePageJson() {
        if (!currentPageId || !currentPageIsEditable()) return;

        let parsed;
        try {
            parsed = JSON.parse(document.getElementById('pageJsonEditor').value);
        } catch (err) {
            setInspectorFeedback(`Invalid page JSON: ${err.message}`, false);
            return;
        }

        parsed.id = currentPageId;
        ensurePageMeta(parsed);

        currentPageDefinition = parsed;
        selectedElementIndex = null;

        try {
            await persistCurrentPageDefinition(true);
            setInspectorFeedback('Raw page JSON saved.', true);
            await loadCatalogPage(currentPageKey);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save raw page JSON.', false);
        }
    }

    function applyElementPatchFromInspector(base) {
        const element = deepClone(base);
        if (!element || typeof element !== 'object') {
            return null;
        }

        const rawName = String(document.getElementById('elementName')?.value ?? '').trim();
        if (rawName.length > 0) {
            element.name = rawName;
        } else {
            delete element.name;
        }

        if (element.type === 'static_text') {
            element.x = parseIntSafe(document.getElementById('elementX')?.value, element.x || 0);
            element.y = parseIntSafe(document.getElementById('elementY')?.value, element.y || 0);
            element.text = String(document.getElementById('elementText')?.value ?? '');
            const textHeightPx = normalizeTextHeightPx(document.getElementById('elementSize')?.value, elementTextHeightPx(element));
            element.text_height_px = textHeightPx;
            element.size = sizeFromTextHeightPx(textHeightPx);
            element.color = monoColorValue(document.getElementById('elementColor')?.value, 'on');
        } else if (element.type === 'dynamic_text') {
            element.x = parseIntSafe(document.getElementById('elementX')?.value, element.x || 0);
            element.y = parseIntSafe(document.getElementById('elementY')?.value, element.y || 0);
            element.source = canonicalDynamicSource(document.getElementById('elementSource')?.value || 'sys:hostname');
            element.prefix = String(document.getElementById('elementPrefix')?.value ?? '');
            element.max_chars = Math.max(0, parseIntSafe(document.getElementById('elementMaxChars')?.value, 20));
            const textHeightPx = normalizeTextHeightPx(document.getElementById('elementSize')?.value, elementTextHeightPx(element));
            element.text_height_px = textHeightPx;
            element.size = sizeFromTextHeightPx(textHeightPx);
            element.color = monoColorValue(document.getElementById('elementColor')?.value, 'on');
        } else if (element.type === 'image') {
            element.x = parseIntSafe(document.getElementById('elementX')?.value, element.x || 0);
            element.y = parseIntSafe(document.getElementById('elementY')?.value, element.y || 0);
            element.w = Math.max(1, parseIntSafe(document.getElementById('elementW')?.value, element.w || 1));
            element.h = Math.max(1, parseIntSafe(document.getElementById('elementH')?.value, element.h || 1));
            element.source = String(document.getElementById('elementSourcePath')?.value ?? '').trim();
            const maskMode = String(document.getElementById('elementMaskMode')?.value || 'alpha');
            if (maskMode === 'alpha' || maskMode === 'alpha_inverted' || maskMode === 'luma_light' || maskMode === 'luma_dark') {
                element.mask_mode = maskMode;
            } else {
                element.mask_mode = 'alpha';
            }
            element.threshold = Math.max(0, Math.min(255, parseIntSafe(document.getElementById('elementThreshold')?.value, 128)));
            element.foreground = monoColorValue(document.getElementById('elementForegroundColor')?.value, 'on');
            element.background = nullableMonoColorValue(document.getElementById('elementBackgroundColor')?.value);
        } else if (element.type === 'rect') {
            element.x = parseIntSafe(document.getElementById('elementX')?.value, element.x || 0);
            element.y = parseIntSafe(document.getElementById('elementY')?.value, element.y || 0);
            element.w = Math.max(1, parseIntSafe(document.getElementById('elementW')?.value, element.w || 1));
            element.h = Math.max(1, parseIntSafe(document.getElementById('elementH')?.value, element.h || 1));
            element.fill = nullableMonoColorValue(document.getElementById('elementFillColor')?.value);
            element.stroke = nullableMonoColorValue(document.getElementById('elementStrokeColor')?.value);
            // Keep legacy field coherent for compatibility tools that still read `filled`.
            element.filled = element.fill === 'on' && element.stroke == null;
        } else if (element.type === 'line') {
            element.x1 = parseIntSafe(document.getElementById('elementX1')?.value, element.x1 || 0);
            element.y1 = parseIntSafe(document.getElementById('elementY1')?.value, element.y1 || 0);
            element.x2 = parseIntSafe(document.getElementById('elementX2')?.value, element.x2 || 0);
            element.y2 = parseIntSafe(document.getElementById('elementY2')?.value, element.y2 || 0);
            element.color = monoColorValue(document.getElementById('elementColor')?.value, 'on');
        }

        return clampElementPosition(element);
    }

    function scheduleElementAutoSave() {
        if (elementAutoSaveTimer) {
            clearTimeout(elementAutoSaveTimer);
        }

        elementAutoSaveTimer = setTimeout(async () => {
            elementAutoSaveTimer = null;
            try {
                await persistCurrentPageDefinition(false);
                setInspectorFeedback('Element changes saved.', true);
            } catch (err) {
                setInspectorFeedback(err.message || 'Failed to save element.', false);
            }
        }, 160);
    }

    function onElementInspectorInputChanged() {
        if (!currentPageDefinition || selectedElementIndex == null || !currentPageIsEditable()) return;
        const current = currentPageDefinition.elements[selectedElementIndex];
        if (!current) return;

        const next = applyElementPatchFromInspector(current);
        if (!next) return;

        currentPageDefinition.elements[selectedElementIndex] = next;
        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();
        updateModeBadge();
        setInspectorFeedback('Saving element changes...', true);
        scheduleElementAutoSave();
    }

    async function applySelectedElementFromInspector() {
        if (!currentPageDefinition || selectedElementIndex == null || !currentPageIsEditable()) return;
        const current = currentPageDefinition.elements[selectedElementIndex];
        if (!current) return;

        const next = applyElementPatchFromInspector(current);
        if (!next) return;

        currentPageDefinition.elements[selectedElementIndex] = next;
        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback('Element changes saved.', true);
            renderInspector();
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save element.', false);
        }
    }

    async function saveSelectedElementJson() {
        if (!currentPageDefinition || selectedElementIndex == null || !currentPageIsEditable()) return;

        let parsed;
        try {
            parsed = JSON.parse(document.getElementById('elementJsonEditor').value);
        } catch (err) {
            setInspectorFeedback(`Invalid element JSON: ${err.message}`, false);
            return;
        }

        if (!parsed || typeof parsed !== 'object' || typeof parsed.type !== 'string') {
            setInspectorFeedback('Element JSON must contain a valid type.', false);
            return;
        }

        currentPageDefinition.elements[selectedElementIndex] = clampElementPosition(parsed);
        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback('Raw element JSON saved.', true);
            renderInspector();
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save raw element JSON.', false);
        }
    }

    function defaultInsertPoint() {
        const pageW = getPageWidth();
        const pageH = getPageHeight();
        return {
            x: Math.max(0, Math.floor(pageW / 2) - 8),
            y: Math.max(0, Math.floor(pageH / 2) - 4),
        };
    }

    async function insertElement(type) {
        if (!currentPageDefinition || !currentPageIsEditable()) return;

        const point = defaultInsertPoint();
        let element;

        if (type === 'static_text') {
            element = {type: 'static_text', x: point.x, y: point.y, text: 'TEXT', size: 'small', text_height_px: 6, color: 'on'};
        } else if (type === 'dynamic_text') {
            element = {
                type: 'dynamic_text',
                x: point.x,
                y: point.y,
                source: 'sys:hostname',
                prefix: '',
                max_chars: 20,
                size: 'small',
                text_height_px: 6,
                color: 'on',
            };
        } else if (type === 'image') {
            element = {
                type: 'image',
                x: point.x,
                y: point.y,
                w: 24,
                h: 24,
                source: '',
                mask_mode: 'alpha',
                threshold: 128,
                foreground: 'on',
                background: null,
            };
        } else if (type === 'rect') {
            element = {type: 'rect', x: point.x, y: point.y, w: 20, h: 10, fill: null, stroke: 'on', filled: false};
        } else if (type === 'line') {
            const pageW = getPageWidth();
            const pageH = getPageHeight();
            element = {
                type: 'line',
                x1: Math.max(0, point.x - 4),
                y1: Math.max(0, point.y - 4),
                x2: Math.min(pageW - 1, point.x + 12),
                y2: Math.min(pageH - 1, point.y + 4),
                color: 'on',
            };
        }

        if (!element) return;

        currentPageDefinition.elements.push(clampElementPosition(element));
        selectedElementIndex = currentPageDefinition.elements.length - 1;

        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();
        renderInspector();

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback(`${elementLabel(element)} added.`, true);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to add element.', false);
        }
    }

    async function moveElementInList(index, direction) {
        if (!currentPageDefinition || !currentPageIsEditable()) return;

        const target = index + direction;
        if (index < 0 || target < 0 || index >= currentPageDefinition.elements.length || target >= currentPageDefinition.elements.length) {
            return;
        }

        const elements = currentPageDefinition.elements;
        [elements[index], elements[target]] = [elements[target], elements[index]];

        if (selectedElementIndex === index) {
            selectedElementIndex = target;
        } else if (selectedElementIndex === target) {
            selectedElementIndex = index;
        }

        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();
        renderInspector();

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback('Element order updated.', true);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to reorder elements.', false);
        }
    }

    async function deleteElement(index) {
        if (!currentPageDefinition || !currentPageIsEditable()) return;
        if (index < 0 || index >= currentPageDefinition.elements.length) return;

        currentPageDefinition.elements.splice(index, 1);
        if (selectedElementIndex != null) {
            if (selectedElementIndex === index) {
                selectedElementIndex = null;
            } else if (selectedElementIndex > index) {
                selectedElementIndex -= 1;
            }
        }

        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();
        renderInspector();

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback('Element deleted.', true);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to delete element.', false);
        }
    }

    function moveElementByDelta(element, dx, dy) {
        const copy = deepClone(element);
        const pageW = getPageWidth();
        const pageH = getPageHeight();

        if (copy.type === 'static_text' || copy.type === 'dynamic_text') {
            copy.x = clamp(parseIntSafe(copy.x) + dx, 0, Math.max(0, pageW - 1));
            copy.y = clamp(parseIntSafe(copy.y) + dy, 0, Math.max(0, pageH - 1));
            return copy;
        }

        if (copy.type === 'rect') {
            copy.w = Math.max(1, parseIntSafe(copy.w, 1));
            copy.h = Math.max(1, parseIntSafe(copy.h, 1));
            copy.x = clamp(parseIntSafe(copy.x) + dx, 0, Math.max(0, pageW - copy.w));
            copy.y = clamp(parseIntSafe(copy.y) + dy, 0, Math.max(0, pageH - copy.h));
            return copy;
        }

        if (copy.type === 'image') {
            copy.w = Math.max(1, parseIntSafe(copy.w, 1));
            copy.h = Math.max(1, parseIntSafe(copy.h, 1));
            copy.x = clamp(parseIntSafe(copy.x) + dx, 0, Math.max(0, pageW - copy.w));
            copy.y = clamp(parseIntSafe(copy.y) + dy, 0, Math.max(0, pageH - copy.h));
            return copy;
        }

        if (copy.type === 'line') {
            const x1 = parseIntSafe(copy.x1);
            const y1 = parseIntSafe(copy.y1);
            const x2 = parseIntSafe(copy.x2);
            const y2 = parseIntSafe(copy.y2);

            const minX = Math.min(x1, x2);
            const maxX = Math.max(x1, x2);
            const minY = Math.min(y1, y2);
            const maxY = Math.max(y1, y2);

            const clampedDx = clamp(dx, -minX, (pageW - 1) - maxX);
            const clampedDy = clamp(dy, -minY, (pageH - 1) - maxY);

            copy.x1 = x1 + clampedDx;
            copy.x2 = x2 + clampedDx;
            copy.y1 = y1 + clampedDy;
            copy.y2 = y2 + clampedDy;
            return copy;
        }

        return copy;
    }

    function beginElementDrag(event, index) {
        if (!currentPageDefinition || !currentPageIsEditable()) return;
        if (index < 0 || index >= currentPageDefinition.elements.length) return;
        if (event.button !== 0) return;

        event.preventDefault();
        event.stopPropagation();

        selectElement(index);

        const pointer = oledCoordFromPointer(event);
        dragState = {
            index,
            pointerId: event.pointerId,
            startX: pointer.x,
            startY: pointer.y,
            original: deepClone(currentPageDefinition.elements[index]),
            moved: false,
        };

        document.getElementById('canvasStage')?.classList.add('is-dragging');

        window.addEventListener('pointermove', onDragMove);
        window.addEventListener('pointerup', onDragEnd);
        window.addEventListener('pointercancel', onDragEnd);
    }

    function onDragMove(event) {
        if (!dragState) return;
        if (event.pointerId !== dragState.pointerId) return;

        const pointer = oledCoordFromPointer(event);
        const dx = pointer.x - dragState.startX;
        const dy = pointer.y - dragState.startY;

        if (dx === 0 && dy === 0) {
            return;
        }

        dragState.moved = true;

        const moved = moveElementByDelta(dragState.original, dx, dy);
        currentPageDefinition.elements[dragState.index] = moved;

        updateCurrentPageDefinitionLocal();
        renderCanvasOverlay();
        renderElements();
        renderInspector();

        updateCanvasHint(`Dragging ${elementDisplayName(moved)} to (${getElementBounds(moved).x}, ${getElementBounds(moved).y})`);
    }

    async function onDragEnd(event) {
        if (!dragState) return;
        if (event.pointerId !== dragState.pointerId) return;

        document.getElementById('canvasStage')?.classList.remove('is-dragging');

        const wasMoved = dragState.moved;
        dragState = null;

        window.removeEventListener('pointermove', onDragMove);
        window.removeEventListener('pointerup', onDragEnd);
        window.removeEventListener('pointercancel', onDragEnd);

        if (!wasMoved) {
            updateCanvasHint('Element selected. Use inspector to edit properties.');
            return;
        }

        try {
            await persistCurrentPageDefinition(false);
            setInspectorFeedback('Element position saved.', true);
        } catch (err) {
            setInspectorFeedback(err.message || 'Failed to save element movement.', false);
        }

        if (selectedElementIndex != null && currentPageDefinition.elements[selectedElementIndex]) {
            const selected = currentPageDefinition.elements[selectedElementIndex];
            updateCanvasHint(`Selected ${elementDisplayName(selected)} #${selectedElementIndex + 1}. Drag on canvas to move.`);
        }
    }

    function buildPageSelectOptions(selectedKey) {
        let options = pageCatalog.map((entry) => {
            const selected = entry.key === selectedKey ? 'selected' : '';
            return `<option value="${escapeHtml(entry.key)}" ${selected}>${escapeHtml(entry.display_name)}</option>`;
        }).join('');

        if (selectedKey && !catalogByKey.has(selectedKey)) {
            options = `<option value="${escapeHtml(selectedKey)}" selected>Missing page (${escapeHtml(selectedKey)})</option>` + options;
        }

        return options;
    }

    function syncRuntimeSpecJsonEditor() {
        document.getElementById('publishedSpecEditor').value = JSON.stringify(runtimeSpecToApiPayload(), null, 2);
    }

    function renderRuntimeEditor() {
        document.getElementById('rotationIntervalInput').value = currentRuntimeSpec.rotation_interval_ms;

        const bootList = document.getElementById('bootSequenceList');
        if (!currentRuntimeSpec.boot_sequence.length) {
            bootList.innerHTML = '<div class="muted">No boot steps configured.</div>';
        } else {
            bootList.innerHTML = currentRuntimeSpec.boot_sequence.map((step, idx) => `
        <div class="runtime-row">
          <div class="runtime-row-top">
            <span class="badge">Step ${idx + 1}</span>
            <select onchange="updateBootPage(${idx}, this.value)">${buildPageSelectOptions(step.page_key)}</select>
          </div>
          <label>
            Duration (ms)
            <input type="number" min="100" step="50" value="${Number(step.duration_ms)}" onchange="updateBootDuration(${idx}, this.value)">
          </label>
          <div class="runtime-row-controls">
            <button class="runtime-mini-btn" onclick="moveBootStepUp(${idx})">Move Up</button>
            <button class="runtime-mini-btn" onclick="moveBootStepDown(${idx})">Move Down</button>
            <button class="runtime-mini-btn danger" onclick="removeBootStep(${idx})">Remove</button>
          </div>
        </div>
      `).join('');
        }

        const rotationList = document.getElementById('rotationQueueList');
        if (!currentRuntimeSpec.rotation_queue.length) {
            rotationList.innerHTML = '<div class="muted">No rotation entries configured.</div>';
        } else {
            rotationList.innerHTML = currentRuntimeSpec.rotation_queue.map((key, idx) => `
        <div class="runtime-row">
          <div class="runtime-row-top">
            <span class="badge">Entry ${idx + 1}</span>
            <select onchange="updateRotationPage(${idx}, this.value)">${buildPageSelectOptions(key)}</select>
          </div>
          <div class="runtime-row-controls">
            <button class="runtime-mini-btn" onclick="moveRotationEntryUp(${idx})">Move Up</button>
            <button class="runtime-mini-btn" onclick="moveRotationEntryDown(${idx})">Move Down</button>
            <button class="runtime-mini-btn danger" onclick="removeRotationEntry(${idx})">Remove</button>
          </div>
        </div>
      `).join('');
        }

        syncRuntimeSpecJsonEditor();

        const errors = validateRuntimeSpec();
        if (errors.length) {
            setRuntimeFeedback(errors.join(' '), false);
        } else {
            setRuntimeFeedback('');
        }
    }

    async function refreshCatalog() {
        const data = await getJson('/api/studio/catalog');
        pageCatalog = Array.isArray(data.pages) ? data.pages : [];
        indexCatalog();
        if (!catalogByKey.has(currentPageKey)) {
            currentPageKey = defaultCatalogKey();
        }
        renderCatalogList();
        renderRuntimeEditor();
    }

    async function loadCatalogPage(pageKey) {
        if (!pageKey) return;

        const detail = await getJson(`/api/studio/catalog/page/${encodeURIComponent(pageKey)}`);
        document.getElementById('canvasStage')?.classList.remove('is-dragging');
        currentPageKey = detail.page.key;
        currentPageId = detail.definition ? detail.definition.id : detail.page.page_id;
        currentPageEntry = detail.page;
        currentPageDefinition = detail.definition || null;
        selectedElementIndex = null;

        renderCatalogList();

        if (detail.definition) {
            ensurePageMeta(currentPageDefinition);
            setEditableState(Boolean(detail.page.editable));
            renderCanvasOverlay();
            renderElements();
            renderInspector();
            updateCanvasHint('Page selected. Click an element on the canvas or in the object list.');
        } else {
            setEditableState(false);
            renderCanvasOverlay();
            renderElements();
            renderInspector();
            updateCanvasHint('No editable page definition loaded for this catalog item.');
        }

        startStudioPreviewStream(currentPageKey);
    }

    async function createPage() {
        const name = document.getElementById('newPageName').value.trim();
        if (!name) {
            alert('Enter a page name first.');
            return;
        }

        const data = await postJson(`/api/studio/pages/create?name=${encodeURIComponent(name)}`);
        document.getElementById('newPageName').value = '';

        await refreshCatalog();
        await loadCatalogPage(data.id);
        setInspectorFeedback('New page created.', true);
    }

    async function deleteCurrentPage() {
        if (!currentPageId || !currentPageIsEditable()) return;
        if (!confirm('Delete this page?')) return;

        const deletingId = currentPageId;
        await postJson(`/api/studio/pages/${encodeURIComponent(deletingId)}/delete`);

        currentPageId = null;
        currentPageEntry = null;
        currentPageDefinition = null;
        selectedElementIndex = null;

        await refreshCatalog();

        if (pageCatalog.length > 0) {
            await loadCatalogPage(defaultCatalogKey());
        } else {
            renderCanvasOverlay();
            renderElements();
            renderInspector();
            setEditableState(false);
            updateCanvasHint('No pages available. Create a new page.');
        }
    }

    async function applyCurrentPage() {
        if (!currentPageId) return;
        await postJson(`/api/studio/pages/${encodeURIComponent(currentPageId)}/apply`);
    }

    async function exportCurrentPageJson() {
        if (!currentPageId) {
            setPageTransferFeedback('No page selected for export.', false);
            return;
        }

        try {
            const raw = await getText(`/api/studio/pages/${encodeURIComponent(currentPageId)}/export`);
            document.getElementById('pageTransferJson').value = raw;
            setPageTransferFeedback(`Exported page '${currentPageId}'.`, true);
        } catch (err) {
            setPageTransferFeedback(err.message || 'Export failed.', false);
        }
    }

    async function copyTransferJson() {
        const raw = document.getElementById('pageTransferJson').value.trim();
        if (!raw) {
            setPageTransferFeedback('Nothing to copy.', false);
            return;
        }
        try {
            await navigator.clipboard.writeText(raw);
            setPageTransferFeedback('Export JSON copied to clipboard.', true);
        } catch (err) {
            setPageTransferFeedback('Clipboard copy failed in this browser/session.', false);
        }
    }

    function downloadTransferJson() {
        const raw = document.getElementById('pageTransferJson').value.trim();
        if (!raw) {
            setPageTransferFeedback('Nothing to download.', false);
            return;
        }

        const filenameBase = (currentPageId || 'oled-page').replace(/[^a-zA-Z0-9._-]/g, '-');
        const blob = new Blob([raw], {type: 'application/json'});
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${filenameBase}.json`;
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
        setPageTransferFeedback('Export JSON downloaded.', true);
    }

    async function importPageJson() {
        const raw = document.getElementById('pageTransferJson').value.trim();
        if (!raw) {
            setPageTransferFeedback('Paste page JSON to import.', false);
            return;
        }

        let parsed;
        try {
            parsed = JSON.parse(raw);
        } catch (err) {
            setPageTransferFeedback(`Invalid JSON: ${err.message}`, false);
            return;
        }

        const conflict = document.getElementById('importConflictPolicy').value;
        try {
            const data = await postJson(`/api/studio/pages/import?conflict=${encodeURIComponent(conflict)}`, parsed, false);
            await refreshCatalog();
            await loadCatalogPage(data.id);
            setPageTransferFeedback(`Imported page as '${data.id}'.`, true);
        } catch (err) {
            setPageTransferFeedback(err.message || 'Import failed.', false);
        }
    }

    function addBootStep() {
        const fallbackKey = catalogByKey.has(currentPageKey) ? currentPageKey : defaultBootKey();
        currentRuntimeSpec.boot_sequence.push({
            page_key: fallbackKey,
            duration_ms: 2000,
        });
        renderRuntimeEditor();
    }

    function updateBootPage(index, key) {
        if (!currentRuntimeSpec.boot_sequence[index]) return;
        currentRuntimeSpec.boot_sequence[index].page_key = key;
        renderRuntimeEditor();
    }

    function updateBootDuration(index, value) {
        if (!currentRuntimeSpec.boot_sequence[index]) return;
        currentRuntimeSpec.boot_sequence[index].duration_ms = Number(value);
        renderRuntimeEditor();
    }

    function moveBootStepUp(index) {
        if (index <= 0 || index >= currentRuntimeSpec.boot_sequence.length) return;
        const items = currentRuntimeSpec.boot_sequence;
        [items[index - 1], items[index]] = [items[index], items[index - 1]];
        renderRuntimeEditor();
    }

    function moveBootStepDown(index) {
        if (index < 0 || index >= currentRuntimeSpec.boot_sequence.length - 1) return;
        const items = currentRuntimeSpec.boot_sequence;
        [items[index], items[index + 1]] = [items[index + 1], items[index]];
        renderRuntimeEditor();
    }

    function removeBootStep(index) {
        if (index < 0 || index >= currentRuntimeSpec.boot_sequence.length) return;
        currentRuntimeSpec.boot_sequence.splice(index, 1);
        renderRuntimeEditor();
    }

    function addRotationEntry() {
        const fallbackKey = catalogByKey.has(currentPageKey) ? currentPageKey : defaultRotationKey();
        currentRuntimeSpec.rotation_queue.push(fallbackKey);
        renderRuntimeEditor();
    }

    function updateRotationPage(index, key) {
        if (!currentRuntimeSpec.rotation_queue[index]) return;
        currentRuntimeSpec.rotation_queue[index] = key;
        renderRuntimeEditor();
    }

    function moveRotationEntryUp(index) {
        if (index <= 0 || index >= currentRuntimeSpec.rotation_queue.length) return;
        const items = currentRuntimeSpec.rotation_queue;
        [items[index - 1], items[index]] = [items[index], items[index - 1]];
        renderRuntimeEditor();
    }

    function moveRotationEntryDown(index) {
        if (index < 0 || index >= currentRuntimeSpec.rotation_queue.length - 1) return;
        const items = currentRuntimeSpec.rotation_queue;
        [items[index], items[index + 1]] = [items[index + 1], items[index]];
        renderRuntimeEditor();
    }

    function removeRotationEntry(index) {
        if (index < 0 || index >= currentRuntimeSpec.rotation_queue.length) return;
        currentRuntimeSpec.rotation_queue.splice(index, 1);
        renderRuntimeEditor();
    }

    function setRotationInterval(value) {
        currentRuntimeSpec.rotation_interval_ms = Number(value);
        renderRuntimeEditor();
    }

    async function savePublishedRuntime() {
        const errors = validateRuntimeSpec();
        if (errors.length) {
            setRuntimeFeedback(errors.join(' '), false);
            return;
        }

        const payload = runtimeSpecToApiPayload();
        await postJson('/api/publish/spec', payload);
        setRuntimeFeedback('Published runtime spec saved.', true);
        syncRuntimeSpecJsonEditor();
    }

    function applyRawPublishedSpecToGui() {
        let parsed;
        try {
            parsed = JSON.parse(document.getElementById('publishedSpecEditor').value);
        } catch (err) {
            setRuntimeFeedback(`Invalid raw published JSON: ${err.message}`, false);
            return;
        }

        currentRuntimeSpec = normalizeRuntimeSpec(parsed);
        renderRuntimeEditor();
        setRuntimeFeedback('Loaded raw JSON into GUI editor.', true);
    }

    async function savePublishedSpecRaw() {
        let parsed;
        try {
            parsed = JSON.parse(document.getElementById('publishedSpecEditor').value);
        } catch (err) {
            setRuntimeFeedback(`Invalid raw published JSON: ${err.message}`, false);
            return;
        }

        await postJson('/api/publish/spec', parsed);
        currentRuntimeSpec = normalizeRuntimeSpec(parsed);
        renderRuntimeEditor();
        setRuntimeFeedback('Raw published spec saved.', true);
    }

    (async function bootStudio() {
        indexCatalog();

        if (!currentPageKey || !catalogByKey.has(currentPageKey)) {
            currentPageKey = defaultCatalogKey();
        }

        currentRuntimeSpec = normalizeRuntimeSpec(INITIAL_PUBLISHED_SPEC);
        if (currentRuntimeSpec.rotation_queue.length === 0 && defaultRotationKey()) {
            currentRuntimeSpec.rotation_queue = [defaultRotationKey()];
        }

        renderCatalogList();
        renderRuntimeEditor();
        renderCanvasOverlay();
        renderElements();
        renderInspector();

        if (currentPageKey) {
            await loadCatalogPage(currentPageKey);
        } else {
            setEditableState(false);
            updateCanvasHint('No pages available. Create a page to start editing.');
        }
    })();

    window.addEventListener('resize', () => {
        renderCanvasOverlay();
    });
