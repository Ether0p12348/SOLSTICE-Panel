    const LED_STUDIO_BOOTSTRAP = window.LED_STUDIO_BOOTSTRAP && typeof window.LED_STUDIO_BOOTSTRAP === 'object'
        ? window.LED_STUDIO_BOOTSTRAP
        : {};
    const LAB_BOOTSTRAP = LED_STUDIO_BOOTSTRAP.labState;

    let labState = LAB_BOOTSTRAP && typeof LAB_BOOTSTRAP === 'object'
        ? LAB_BOOTSTRAP
        : { entries: [] };
    let runtimeState = null;

    let selectedModeId = null;
    let selectedSpeedValue = null;
    let selectedColorValue = null;

    function escapeHtml(value) {
        return String(value || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function toHex(n) {
        const value = Number(n) || 0;
        return `0x${value.toString(16).padStart(2, '0')}`;
    }

    function setFeedback(message, kind = '') {
        const el = document.getElementById('studioFeedback');
        el.textContent = message || '';
        el.className = 'feedback';
        if (kind) el.classList.add(kind);
    }

    async function getJson(url) {
        const res = await fetch(url, { cache: 'no-store' });
        const raw = await res.text();
        let data;
        try {
            data = raw ? JSON.parse(raw) : {};
        } catch {
            throw new Error(raw || `Request failed (${res.status})`);
        }
        if (!res.ok) {
            throw new Error(data.error || `Request failed (${res.status})`);
        }
        return data;
    }

    async function postJson(url, payload = null) {
        const options = { method: 'POST', headers: {} };
        if (payload !== null) {
            options.headers['Content-Type'] = 'application/json';
            options.body = JSON.stringify(payload);
        }
        const res = await fetch(url, options);
        const raw = await res.text();
        let data;
        try {
            data = raw ? JSON.parse(raw) : {};
        } catch {
            throw new Error(raw || `Request failed (${res.status})`);
        }
        if (!res.ok || data.ok === false) {
            throw new Error(data.error || `Request failed (${res.status})`);
        }
        return data;
    }

    function entriesByClass(commandClass) {
        return (Array.isArray(labState.entries) ? labState.entries : [])
            .filter((entry) => String(entry.command_class || '') === commandClass)
            .sort((a, b) => Number(a.value) - Number(b.value));
    }

    function speedEntries() {
        return entriesByClass('builtin_speed')
            .filter((entry) => Number(entry.register) === 0x05)
            .filter((entry) => Number(entry.value) >= 1 && Number(entry.value) <= 3);
    }

    function colorEntries() {
        return entriesByClass('builtin_color')
            .filter((entry) => Number(entry.register) === 0x06)
            .filter((entry) => Number(entry.value) >= 0 && Number(entry.value) <= 6);
    }

    function studioModes() {
        return entriesByClass('builtin_effect')
            .filter((entry) => Number(entry.register) === 0x04)
            .slice()
            .sort((a, b) => Number(a.value) - Number(b.value));
    }

    function selectedMode() {
        return studioModes().find((mode) => String(mode.id) === String(selectedModeId)) || null;
    }

    function modeValueFromMode(mode) {
        if (!mode) return 1;
        const raw = Number(mode.value);
        return Number.isFinite(raw) ? raw : 1;
    }

    function renderModeGrid() {
        const container = document.getElementById('modeGrid');
        const modes = studioModes();
        if (!modes.length) {
            container.innerHTML = '<div class="muted">No built-in effect commands found in LED-lab.</div>';
            return;
        }

        container.innerHTML = modes.map((mode) => {
            const active = String(selectedModeId) === String(mode.id) ? 'active' : '';
            const chips = [
                mode.speed_customization_available ? 'speed' : null,
                mode.color_preset_customization_available ? 'color' : null,
                mode.can_index ? 'index' : null,
                mode.can_color_24 ? '24-bit' : null,
            ].filter(Boolean).join(', ') || 'fixed';
            return `
                <button class="mode-card ${active}" type="button" onclick="selectMode('${escapeHtml(mode.id)}')">
                    <div class="mode-title">${escapeHtml(mode.label || mode.id)}</div>
                    <div class="mode-meta">${toHex(mode.register)} = ${toHex(mode.value)} · options: ${escapeHtml(chips)}</div>
                    <div class="mode-desc">${escapeHtml(mode.description || '')}</div>
                </button>
            `;
        }).join('');
    }

    function renderValueGrid(elementId, entries, selectedValue, onClick) {
        const container = document.getElementById(elementId);
        if (!entries.length) {
            container.innerHTML = '<div class="muted">No labeled values available.</div>';
            return;
        }
        container.innerHTML = entries.map((entry) => {
            const value = Number(entry.value);
            const active = Number(selectedValue) === value ? 'active' : '';
            return `
                <button class="value-chip ${active}" type="button" onclick="${onClick}(${value})">
                    <div><strong>${escapeHtml(entry.label || `Value ${value}`)}</strong></div>
                    <div class="value-meta">${toHex(entry.register)} = ${toHex(value)}</div>
                </button>
            `;
        }).join('');
    }

    function renderCustomPanel() {
        const panel = document.getElementById('customPanel');
        const caption = document.getElementById('customModeCaption');
        const speedSection = document.getElementById('speedSection');
        const colorSection = document.getElementById('colorSection');
        const directSection = document.getElementById('directSection');

        const mode = selectedMode();
        if (!mode) {
            panel.classList.remove('show');
            return;
        }

        const showSpeed = Boolean(mode.can_speed);
        const showColor = Boolean(mode.can_color_preset);
        const showDirect = Boolean(mode.can_index) && Boolean(mode.can_color_24);

        if (!showSpeed && !showColor && !showDirect) {
            panel.classList.remove('show');
            return;
        }

        panel.classList.add('show');
        caption.textContent = `${mode.label || mode.id} (${toHex(mode.register)}=${toHex(mode.value)})`;

        speedSection.style.display = showSpeed ? 'block' : 'none';
        colorSection.style.display = showColor ? 'block' : 'none';
        directSection.style.display = showDirect ? 'block' : 'none';

        if (showSpeed) {
            renderValueGrid('speedGrid', speedEntries(), selectedSpeedValue, 'selectSpeed');
        }
        if (showColor) {
            renderValueGrid('colorGrid', colorEntries(), selectedColorValue, 'selectColor');
        }
    }

    function renderRuntimePane() {
        const pane = document.getElementById('runtimePane');
        if (!runtimeState || typeof runtimeState !== 'object') {
            pane.textContent = 'Runtime status unavailable.';
            return;
        }

        const controller = runtimeState.controller_mode || {};
        pane.textContent = [
            `playing: ${runtimeState.playing ? 'yes' : 'no'}`,
            `controller override: ${runtimeState.controller_mode_enabled ? 'enabled' : 'disabled'}`,
            `mode/speed/color: ${controller.mode ?? '?'} / ${controller.speed ?? '?'} / ${controller.color_index ?? '?'}`,
        ].join('\n');

        document.getElementById('overrideToggle').checked = Boolean(runtimeState.controller_mode_enabled);
    }

    function renderAll() {
        renderModeGrid();
        renderCustomPanel();
        renderRuntimePane();
    }

    function syncSelectionFromRuntime() {
        const controller = runtimeState?.controller_mode || {};
        const modes = studioModes();

        if (selectedModeId === null && Number.isFinite(Number(controller.mode))) {
            const modeFromRuntime = modes.find((mode) => modeValueFromMode(mode) === Number(controller.mode));
            if (modeFromRuntime) {
                selectedModeId = modeFromRuntime.id;
            }
        }
        if (selectedModeId === null && modes.length) {
            selectedModeId = modes[0].id;
        }

        if (selectedSpeedValue === null && Number.isFinite(Number(controller.speed))) {
            selectedSpeedValue = Number(controller.speed);
        }
        if (selectedColorValue === null && Number.isFinite(Number(controller.color_index))) {
            selectedColorValue = Number(controller.color_index);
        }

        if (selectedSpeedValue === null) selectedSpeedValue = 2;
        if (selectedColorValue === null) selectedColorValue = 0;
    }

    function selectMode(id) {
        selectedModeId = String(id);
        renderAll();
    }

    function selectSpeed(value) {
        selectedSpeedValue = Number(value);
        renderAll();
    }

    function selectColor(value) {
        selectedColorValue = Number(value);
        renderAll();
    }

    async function refreshLabState() {
        labState = await getJson('/api/led/lab/state');
    }

    async function refreshRuntimeState() {
        runtimeState = await getJson('/api/led/runtime');
        syncSelectionFromRuntime();
    }

    async function refreshAll() {
        try {
            await refreshLabState();
            await refreshRuntimeState();
            renderAll();
            setFeedback('LED Studio refreshed.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to refresh LED Studio state.', 'error');
        }
    }

    async function applySelectedMode() {
        const mode = selectedMode();
        if (!mode) {
            setFeedback('No mode selected.', 'error');
            return;
        }

        try {
            const payload = {
                enabled: true,
                mode: modeValueFromMode(mode),
                speed: Number(selectedSpeedValue),
                color_index: Number(selectedColorValue),
            };
            const data = await postJson('/api/led/runtime/controller_mode', payload);
            runtimeState = data.runtime || runtimeState;
            syncSelectionFromRuntime();
            renderAll();
            setFeedback(`Applied mode ${mode.label || mode.id}.`, 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to apply selected mode.', 'error');
        }
    }

    async function writeDirectPixel() {
        const mode = selectedMode();
        if (!mode || !mode.can_index || !mode.can_color_24) {
            setFeedback('Direct pixel programming is only available for modes with index + 24-bit support.', 'error');
            return;
        }

        const index = Number.parseInt(document.getElementById('directPixelIndex').value, 10);
        const hex = String(document.getElementById('directPixelHex').value || '').trim();
        if (!Number.isInteger(index) || index < 0 || index > 13) {
            setFeedback('LED index must be between 0 and 13.', 'error');
            return;
        }
        if (!/^#?[0-9a-fA-F]{6}$/.test(hex)) {
            setFeedback('HEX color must be 6 hex characters, e.g. 00ff88.', 'error');
            return;
        }

        try {
            await applySelectedMode();
            await postJson('/api/led/runtime/direct_pixel', {
                index,
                hex: hex.replace(/^#/, ''),
            });
            setFeedback(`Direct pixel updated: LED ${index} = #${hex.replace(/^#/, '').toUpperCase()}`, 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to write direct pixel.', 'error');
        }
    }

    async function disableOverride() {
        try {
            const data = await postJson('/api/led/runtime/controller_mode', { enabled: false });
            runtimeState = data.runtime || runtimeState;
            renderAll();
            setFeedback('Controller override disabled.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to disable controller override.', 'error');
        }
    }

    async function playRuntime() {
        try {
            const data = await postJson('/api/led/runtime/play', {});
            runtimeState = data.runtime || runtimeState;
            renderAll();
            setFeedback('LED runtime playing.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to play runtime.', 'error');
        }
    }

    async function pauseRuntime() {
        try {
            const data = await postJson('/api/led/runtime/pause', {});
            runtimeState = data.runtime || runtimeState;
            renderAll();
            setFeedback('LED runtime paused.', 'ok');
        } catch (err) {
            setFeedback(err.message || 'Failed to pause runtime.', 'error');
        }
    }

    document.getElementById('overrideToggle').addEventListener('change', async (event) => {
        const enabled = event.target.checked;
        try {
            if (enabled) {
                await applySelectedMode();
            } else {
                await disableOverride();
            }
        } catch {
            // feedback already handled in called functions
        }
    });

    (function boot() {
        refreshAll();
        setInterval(async () => {
            try {
                await refreshRuntimeState();
                renderRuntimePane();
            } catch {
                // ignore transient polling errors
            }
        }, 2000);
    })();
