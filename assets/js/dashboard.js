    const DASHBOARD_BOOTSTRAP = window.DASHBOARD_BOOTSTRAP && typeof window.DASHBOARD_BOOTSTRAP === 'object'
        ? window.DASHBOARD_BOOTSTRAP
        : {};
    const PAGE_CATALOG_BOOTSTRAP = DASHBOARD_BOOTSTRAP.pageCatalog;
    const PUBLISHED_SPEC = DASHBOARD_BOOTSTRAP.publishedSpec && typeof DASHBOARD_BOOTSTRAP.publishedSpec === 'object'
        ? DASHBOARD_BOOTSTRAP.publishedSpec
        : {};
    const INITIAL_SNAPSHOT = DASHBOARD_BOOTSTRAP.initialSnapshot && typeof DASHBOARD_BOOTSTRAP.initialSnapshot === 'object'
        ? DASHBOARD_BOOTSTRAP.initialSnapshot
        : {
            active_page: '',
            display_mode: 'unknown',
            rotation_interval_ms: 5000,
            rotation_queue: [],
            rotation_active: false,
            rotation_queue_len: 0,
            rotation_index: null,
            hostname: 'loading...',
            ip_addr: 'loading...',
            uptime_text: 'loading...',
            ram_percent_text: 'loading...',
            cpu_temp_text: 'loading...',
            standby_active: false,
            led_requested_on: true,
            led_effective_on: true,
            fan_requested_on: true,
            fan_effective_on: true,
            fan_auto_forced_by_temp: false,
            fan_explicit_off_temp_warning: false,
            fan_last_error: null,
            fan_auto_on_temp_c: 70,
        };
    let pageCatalog = Array.isArray(PAGE_CATALOG_BOOTSTRAP) ? PAGE_CATALOG_BOOTSTRAP.slice() : [];
    let catalogByKey = new Map(pageCatalog.map((entry) => [entry.key, entry]));
    const LEGACY_ID_ALIASES = {
        boot: 'solstice-panel-core-1.0.0-boot',
        live_info: 'solstice-panel-core-1.0.0-live-info',
        diagnostics: 'solstice-panel-core-1.0.0-diagnostics',
    };
    let rotationIntervalApplyTimer = null;
    let fanThresholdApplyTimer = null;
    let powerStatePollInFlight = false;
    let powerStatePollTimer = null;
    let pageCatalogPollInFlight = false;
    let pageCatalogPollTimer = null;

    function normalizeCatalogKey(key) {
        if (!key || typeof key !== 'string') return '';
        let out = key;
        if (out.startsWith('custom:')) out = out.slice('custom:'.length);
        if (out.startsWith('system:')) out = out.slice('system:'.length);
        return LEGACY_ID_ALIASES[out] || out;
    }

    function runtimeLabelToCatalogKey(label) {
        return normalizeCatalogKey(label);
    }

    function pageRefToCatalogKey(pageRef) {
        if (typeof pageRef === 'string') return normalizeCatalogKey(pageRef);
        if (pageRef && typeof pageRef === 'object' && typeof pageRef.value === 'string') {
            return normalizeCatalogKey(pageRef.value);
        }
        return '';
    }

    function displayNameForCatalogKey(key) {
        if (!key) return 'unknown';
        const entry = catalogByKey.get(key);
        if (!entry) return key;
        return `${entry.display_name} (${entry.page_id || key})`;
    }

    function displayNameForRuntimeLabel(label) {
        return displayNameForCatalogKey(runtimeLabelToCatalogKey(label));
    }

    function syncCatalogCache(entries, preserveSelection = true) {
        if (!Array.isArray(entries)) return;
        const select = document.getElementById('livePageSelect');
        const previous = preserveSelection && select ? String(select.value || '') : '';
        pageCatalog = entries.filter((entry) => entry && typeof entry.key === 'string');
        catalogByKey = new Map(pageCatalog.map((entry) => [entry.key, entry]));

        if (!select) return;
        const options = pageCatalog.map((entry) => (
            `<option value="${entry.key}">${entry.display_name}</option>`
        )).join('');
        select.innerHTML = options;

        if (previous && catalogByKey.has(previous)) {
            select.value = previous;
        } else if (pageCatalog.length > 0) {
            select.value = pageCatalog[0].key;
        }
    }

    function renderPageSelect() {
        const select = document.getElementById('livePageSelect');
        if (!select) return;
        syncCatalogCache(pageCatalog, true);
    }

    async function refreshPageCatalog() {
        if (pageCatalogPollInFlight) return;
        pageCatalogPollInFlight = true;
        try {
            const res = await fetch('/api/studio/catalog', {cache: 'no-store'});
            if (!res.ok) {
                throw new Error(`HTTP ${res.status}`);
            }
            const data = await res.json();
            if (data && Array.isArray(data.pages)) {
                const preserve = document.activeElement !== document.getElementById('livePageSelect');
                syncCatalogCache(data.pages, preserve);
            }
        } catch (err) {
            console.error('failed to refresh page catalog:', err);
        } finally {
            pageCatalogPollInFlight = false;
        }
    }

    function setFeedback(message, kind = '') {
        const el = document.getElementById('runtimeFeedback');
        el.textContent = message || '';
        el.className = 'runtime-feedback';
        if (kind) {
            el.classList.add(kind);
        }
    }

    function setSafetyFeedback(message, kind = '') {
        const el = document.getElementById('safetyFeedback');
        el.textContent = message || '';
        el.className = 'runtime-feedback';
        if (kind) {
            el.classList.add(kind);
        }
    }

    function renderPublishedSummary() {
        const intervalEl = document.getElementById('publishedIntervalStatus');
        const bootList = document.getElementById('bootSummaryList');
        const rotationList = document.getElementById('rotationSummaryList');

        intervalEl.textContent = `${PUBLISHED_SPEC.rotation_interval_ms || 0} ms`;

        const bootSequence = Array.isArray(PUBLISHED_SPEC.boot_sequence) ? PUBLISHED_SPEC.boot_sequence : [];
        if (bootSequence.length === 0) {
            bootList.innerHTML = '<li class="muted">No boot steps configured.</li>';
        } else {
            bootList.innerHTML = bootSequence.map((step) => {
                const key = pageRefToCatalogKey(step.page_id ?? step.page_ref);
                const name = displayNameForCatalogKey(key);
                const duration = Number(step.duration_ms) || 0;
                return `<li><strong>${name}</strong> for ${duration} ms</li>`;
            }).join('');
        }

        const rotationQueue = Array.isArray(PUBLISHED_SPEC.rotation_queue) ? PUBLISHED_SPEC.rotation_queue : [];
        if (rotationQueue.length === 0) {
            rotationList.innerHTML = '<li class="muted">No rotation queue entries configured.</li>';
        } else {
            rotationList.innerHTML = rotationQueue.map((pageRef) => {
                const key = pageRefToCatalogKey(pageRef);
                const name = displayNameForCatalogKey(key);
                return `<li>${name}</li>`;
            }).join('');
        }
    }

    function applyPowerState(power) {
        if (!power || typeof power !== 'object') return;

        const standbyActive = Boolean(power.standby_active);
        const ledRequestedOn = Boolean(power.led_requested_on);
        const ledEffectiveOn = Boolean(power.led_effective_on);
        const fanRequestedOn = Boolean(power.fan_requested_on);
        const fanEffectiveOn = Boolean(power.fan_effective_on);
        const fanForcedByTemp = Boolean(power.fan_auto_forced_by_temp);
        const fanWarning = Boolean(power.fan_explicit_off_temp_warning);
        const fanError = power.fan_last_error ? String(power.fan_last_error) : '';
        const threshold = Number.parseInt(power.fan_auto_on_temp_c, 10);

        const standbyToggle = document.getElementById('standbyToggle');
        const ledToggle = document.getElementById('ledToggle');
        const fanToggle = document.getElementById('fanToggle');
        if (standbyToggle) standbyToggle.checked = standbyActive;
        if (ledToggle) ledToggle.checked = ledRequestedOn;
        if (fanToggle) fanToggle.checked = fanRequestedOn;

        const thresholdInput = document.getElementById('fanAutoTemp');
        if (thresholdInput && document.activeElement !== thresholdInput && Number.isFinite(threshold)) {
            thresholdInput.value = threshold;
        }

        const standbyChip = document.getElementById('standbyStatus');
        standbyChip.textContent = standbyActive ? 'active' : 'inactive';
        standbyChip.className = standbyActive ? 'status-chip warn' : 'status-chip';

        const ledRequestedChip = document.getElementById('ledRequestedStatus');
        ledRequestedChip.textContent = ledRequestedOn ? 'on' : 'off';
        ledRequestedChip.className = ledRequestedOn ? 'status-chip ok' : 'status-chip';

        const ledEffectiveChip = document.getElementById('ledEffectiveStatus');
        ledEffectiveChip.textContent = ledEffectiveOn ? 'on' : 'off';
        ledEffectiveChip.className = ledEffectiveOn ? 'status-chip ok' : 'status-chip';

        const fanRequestedChip = document.getElementById('fanRequestedStatus');
        fanRequestedChip.textContent = fanRequestedOn ? 'on' : 'off';
        fanRequestedChip.className = fanRequestedOn ? 'status-chip ok' : 'status-chip';

        const fanEffectiveChip = document.getElementById('fanEffectiveStatus');
        fanEffectiveChip.textContent = fanEffectiveOn ? 'on' : 'off';
        fanEffectiveChip.className = fanEffectiveOn ? 'status-chip ok' : 'status-chip';

        const fanProtectionChip = document.getElementById('fanProtectionStatus');
        if (fanForcedByTemp) {
            fanProtectionChip.textContent = `auto-on in standby (>= ${Number.isFinite(threshold) ? threshold : '?'}C)`;
            fanProtectionChip.className = 'status-chip ok';
            setSafetyFeedback('Standby protection active: fan auto-enabled due to CPU temperature.', 'ok');
        } else if (fanWarning) {
            fanProtectionChip.textContent = `warning: fan off while CPU hot (>= ${Number.isFinite(threshold) ? threshold : '?'}C)`;
            fanProtectionChip.className = 'status-chip warn';
            setSafetyFeedback('Warning: fan is explicitly off while CPU temperature is above threshold.', 'warn');
        } else {
            fanProtectionChip.textContent = 'normal';
            fanProtectionChip.className = 'status-chip';
            setSafetyFeedback('', '');
        }

        document.getElementById('fanErrorStatus').textContent = fanError || 'none';
    }

    function applySnapshot(snapshot) {
        document.getElementById('activePageStatus').textContent = displayNameForRuntimeLabel(snapshot.active_page);
        document.getElementById('displayModeStatus').textContent = snapshot.display_mode || 'unknown';
        document.getElementById('rotationIntervalStatus').textContent = `${snapshot.rotation_interval_ms} ms`;
        document.getElementById('queueLengthStatus').textContent = `${snapshot.rotation_queue_len || 0} pages`;

        const hasQueueIndex = typeof snapshot.rotation_index === 'number' && snapshot.rotation_queue_len > 0;
        document.getElementById('queueIndexStatus').textContent = hasQueueIndex
            ? `${snapshot.rotation_index + 1} / ${snapshot.rotation_queue_len}`
            : '-';

        const queueNames = Array.isArray(snapshot.rotation_queue)
            ? snapshot.rotation_queue.map((label) => displayNameForRuntimeLabel(label))
            : [];
        document.getElementById('queueSummaryStatus').textContent = queueNames.length
            ? queueNames.join('  ->  ')
            : '-';

        document.getElementById('hostnameStatus').textContent = snapshot.hostname || 'N/A';
        document.getElementById('ipStatus').textContent = snapshot.ip_addr || 'N/A';
        document.getElementById('uptimeStatus').textContent = snapshot.uptime_text || 'N/A';
        document.getElementById('ramStatus').textContent = snapshot.ram_percent_text || 'N/A';
        document.getElementById('cpuStatus').textContent = snapshot.cpu_temp_text || 'N/A';

        const rotationActive = typeof snapshot.rotation_active === 'boolean'
            ? snapshot.rotation_active
            : snapshot.display_mode === 'rotating';

        const rotationChip = document.getElementById('rotationActiveStatus');
        rotationChip.textContent = rotationActive ? 'active' : 'paused';
        rotationChip.className = rotationActive ? 'status-chip ok' : 'status-chip';

        const rotationToggle = document.getElementById('rotationToggle');
        rotationToggle.checked = rotationActive;

        const rotationInput = document.getElementById('rotationMs');
        if (document.activeElement !== rotationInput) {
            rotationInput.value = snapshot.rotation_interval_ms;
        }

        const activeKey = runtimeLabelToCatalogKey(snapshot.active_page);
        const livePageSelect = document.getElementById('livePageSelect');
        if (catalogByKey.has(activeKey) && document.activeElement !== livePageSelect) {
            livePageSelect.value = activeKey;
        }

        applyPowerState(snapshot);
    }

    function drawPreviewFrame(frame) {
        const canvas = document.getElementById('oledPreview');
        if (canvas.width !== frame.width) canvas.width = frame.width;
        if (canvas.height !== frame.height) canvas.height = frame.height;

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
    }

    async function refreshPowerState() {
        if (powerStatePollInFlight) return;
        powerStatePollInFlight = true;
        try {
            const res = await fetch('/api/power', {cache: 'no-store'});
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            applyPowerState(await res.json());
        } catch (err) {
            console.error('failed to refresh power state:', err);
        } finally {
            powerStatePollInFlight = false;
        }
    }

    async function postJson(url, body = null) {
        const options = {method: 'POST', headers: {}};

        if (body !== null) {
            options.headers['Content-Type'] = 'application/json';
            options.body = JSON.stringify(body);
        }

        const res = await fetch(url, options);
        const data = await res.json();
        if (!res.ok) {
            throw new Error(data.error || 'Request failed');
        }
        return data;
    }

    async function toggleRotation(enabled) {
        const url = enabled
            ? '/api/display/rotation/enable'
            : '/api/display/rotation/disable';
        const data = await postJson(url);
        if (data.snapshot) applySnapshot(data.snapshot);
        if (data.preview) drawPreviewFrame(data.preview.frame);
    }

    async function toggleStandby(enabled) {
        const data = await postJson(`/api/power/standby?enabled=${enabled ? 'true' : 'false'}`);
        if (data.power) applyPowerState(data.power);
    }

    async function toggleFan(enabled) {
        const data = await postJson(`/api/power/fan?enabled=${enabled ? 'true' : 'false'}`);
        if (data.power) applyPowerState(data.power);
    }

    async function toggleLed(enabled) {
        const data = await postJson(`/api/power/led?enabled=${enabled ? 'true' : 'false'}`);
        if (data.power) applyPowerState(data.power);
    }

    async function applyFanAutoThreshold() {
        const input = document.getElementById('fanAutoTemp');
        const c = Number.parseInt(input.value, 10);
        if (!Number.isFinite(c) || c < 30 || c > 110) {
            setFeedback('Fan threshold must be between 30C and 110C.', 'error');
            return;
        }
        try {
            const data = await postJson(`/api/power/fan/auto_threshold?c=${encodeURIComponent(c)}`);
            if (data.power) applyPowerState(data.power);
            setFeedback('Fan auto-on threshold saved.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to save fan threshold.', 'error');
        }
    }

    async function showSelectedPageNow() {
        const key = document.getElementById('livePageSelect').value;
        if (!key) {
            setFeedback('Invalid page selection.', 'error');
            return;
        }

        try {
            const data = await postJson('/api/display/runtime_page', {page_id: key});
            if (data.snapshot) applySnapshot(data.snapshot);
            if (data.preview) drawPreviewFrame(data.preview.frame);
            setFeedback('Temporary live page override applied.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to set live page override.', 'error');
        }
    }

    async function applyTemporaryRotationInterval() {
        const input = document.getElementById('rotationMs');
        const ms = Number.parseInt(input.value, 10);

        if (!Number.isFinite(ms) || ms < 250) {
            setFeedback('Rotation interval must be at least 250 ms.', 'error');
            return;
        }

        try {
            const data = await postJson(`/api/display/rotation/interval?ms=${encodeURIComponent(ms)}`);
            if (data.snapshot) applySnapshot(data.snapshot);
            if (data.preview) drawPreviewFrame(data.preview.frame);
            setFeedback('Temporary rotation interval applied.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to set temporary rotation interval.', 'error');
        }
    }

    async function resumePublishedRotation() {
        try {
            const data = await postJson('/api/display/rotation/resume_published');
            if (data.snapshot) applySnapshot(data.snapshot);
            if (data.preview) drawPreviewFrame(data.preview.frame);
            setFeedback('Live runtime returned to published rotation schedule.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to resume published rotation.', 'error');
        }
    }

    const eventSource = new EventSource('/api/events');

    eventSource.addEventListener('status', (event) => {
        try {
            applySnapshot(JSON.parse(event.data));
        } catch (err) {
            console.error('failed to parse status SSE payload:', err);
        }
    });

    eventSource.addEventListener('preview', (event) => {
        try {
            const snapshot = JSON.parse(event.data);
            drawPreviewFrame(snapshot.frame);
        } catch (err) {
            console.error('failed to parse preview SSE payload:', err);
        }
    });

    eventSource.onerror = (err) => {
        console.error('SSE error:', err);
    };

    document.getElementById('rotationToggle').addEventListener('change', async (event) => {
        const desired = event.target.checked;
        try {
            await toggleRotation(desired);
            setFeedback(desired ? 'Rotation enabled.' : 'Rotation paused.', 'ok');
        } catch (err) {
            event.target.checked = !desired;
            setFeedback(err.message || 'Failed to change rotation mode.', 'error');
        }
    });

    document.getElementById('standbyToggle').addEventListener('change', async (event) => {
        const desired = event.target.checked;
        try {
            await toggleStandby(desired);
            setFeedback(desired ? 'Standby enabled: LEDs and fan follow standby safety rules.' : 'Standby disabled.', 'ok');
        } catch (err) {
            event.target.checked = !desired;
            setFeedback(err.message || 'Failed to toggle standby mode.', 'error');
        }
    });

    document.getElementById('fanToggle').addEventListener('change', async (event) => {
        const desired = event.target.checked;
        try {
            await toggleFan(desired);
            setFeedback(desired ? 'Fan requested ON.' : 'Fan requested OFF.', 'ok');
        } catch (err) {
            event.target.checked = !desired;
            setFeedback(err.message || 'Failed to toggle fan power.', 'error');
        }
    });

    document.getElementById('ledToggle').addEventListener('change', async (event) => {
        const desired = event.target.checked;
        try {
            await toggleLed(desired);
            setFeedback(desired ? 'LEDs requested ON.' : 'LEDs requested OFF.', 'ok');
        } catch (err) {
            event.target.checked = !desired;
            setFeedback(err.message || 'Failed to toggle LED power.', 'error');
        }
    });

    document.getElementById('livePageSelect').addEventListener('change', async () => {
        await showSelectedPageNow();
    });

    document.getElementById('rotationMs').addEventListener('input', () => {
        if (rotationIntervalApplyTimer) {
            clearTimeout(rotationIntervalApplyTimer);
        }
        rotationIntervalApplyTimer = setTimeout(async () => {
            rotationIntervalApplyTimer = null;
            await applyTemporaryRotationInterval();
        }, 220);
    });

    document.getElementById('fanAutoTemp').addEventListener('input', () => {
        if (fanThresholdApplyTimer) {
            clearTimeout(fanThresholdApplyTimer);
        }
        fanThresholdApplyTimer = setTimeout(async () => {
            fanThresholdApplyTimer = null;
            await applyFanAutoThreshold();
        }, 260);
    });

    renderPageSelect();
    refreshPageCatalog();
    renderPublishedSummary();
    refreshPowerState();
    powerStatePollTimer = setInterval(refreshPowerState, 1000);
    pageCatalogPollTimer = setInterval(refreshPageCatalog, 10000);
    applySnapshot(INITIAL_SNAPSHOT);
