    const CONFIGURATION_BOOTSTRAP = window.CONFIGURATION_BOOTSTRAP && typeof window.CONFIGURATION_BOOTSTRAP === 'object'
        ? window.CONFIGURATION_BOOTSTRAP
        : {};
    const INITIAL_CONFIG_SCHEMA = CONFIGURATION_BOOTSTRAP.configSchema;
    let configSchema = INITIAL_CONFIG_SCHEMA && Array.isArray(INITIAL_CONFIG_SCHEMA.sections)
        ? INITIAL_CONFIG_SCHEMA
        : {sections: []};
    const DYNAMIC_I2C_ADDRESS_KEYS = new Set(['display.address', 'led.address']);

    function escapeHtml(value) {
        return String(value)
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    async function getJson(url) {
        const res = await fetch(url);
        const data = await res.json();
        if (!res.ok) {
            throw new Error(data.error || 'Request failed');
        }
        return data;
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

    async function postText(url, text) {
        const res = await fetch(url, {
            method: 'POST',
            headers: {'Content-Type': 'text/plain;charset=UTF-8'},
            body: text,
        });
        const data = await res.json();
        if (!res.ok) {
            throw new Error(data.error || 'Request failed');
        }
        return data;
    }

    function setSystemConfigFeedback(message, ok = false) {
        const el = document.getElementById('systemConfigFeedback');
        el.textContent = message;
        el.className = ok ? 'runtime-feedback ok' : 'runtime-feedback error';
        if (!message) {
            el.className = 'runtime-feedback muted';
        }
    }

    function renderConfigSchema() {
        const container = document.getElementById('configSchemaSections');
        if (!configSchema || !Array.isArray(configSchema.sections) || configSchema.sections.length === 0) {
            container.innerHTML = '<div class="muted">No system configuration fields available.</div>';
            return;
        }

        container.innerHTML = configSchema.sections.map((section) => {
            const fields = Array.isArray(section.fields) ? section.fields : [];
            const regularFields = fields.filter((field) => !field.advanced);
            const advancedFields = fields.filter((field) => field.advanced);

            const regularFieldsHtml = regularFields.length
                ? regularFields.map((field) => renderConfigField(field)).join('')
                : '';
            const advancedFieldsHtml = advancedFields.length
                ? `
            <details class="section-advanced">
              <summary>Advanced Settings (${advancedFields.length})</summary>
              <div class="config-field-list">${advancedFields.map((field) => renderConfigField(field)).join('')}</div>
            </details>
          `
                : '';
            const fieldsHtml = (regularFieldsHtml || advancedFieldsHtml)
                ? `${regularFieldsHtml}${advancedFieldsHtml}`
                : '<div class="muted">No editable fields in this section.</div>';

            return `
        <div class="card config-section-card">
          <h3 style="margin: 0;">${escapeHtml(section.label || section.id || 'Section')}</h3>
          <p class="config-section-meta">${escapeHtml(section.description || '')}</p>
          <div class="config-field-list">${fieldsHtml}</div>
        </div>
      `;
        }).join('');
    }

    function renderConfigField(field) {
        const key = String(field.key || '');
        const rawLabel = String(field.label || key);
        const restartRequired = rawLabel.startsWith('*');
        const label = restartRequired ? rawLabel.slice(1).trim() : rawLabel;
        const description = String(field.description || '');
        const type = String(field.field_type || 'text');
        const titleHtml = restartRequired
            ? `<span class="restart-flag" title="Requires panel restart">*</span>${escapeHtml(label)}`
            : escapeHtml(label);
        const advancedChip = field.advanced ? '<span class="badge">Advanced</span>' : '';
        const readOnlyChip = field.read_only ? '<span class="badge">Read-only</span>' : '';
        const chips = (advancedChip || readOnlyChip)
            ? `<div class="row">${advancedChip}${readOnlyChip}</div>`
            : '';
        const fieldClasses = `config-field${field.advanced ? ' config-field-advanced' : ''}`;
        const descriptionHtml = escapeHtml(description);

        let controlHtml = '';
        if (type === 'boolean') {
            const checked = field.value === true ? 'checked' : '';
            const disabled = field.read_only ? 'disabled' : '';
            controlHtml = `
        <div class="config-bool-row">
          <span class="muted">Enabled</span>
          <label class="switch" title="Toggle ${escapeHtml(label)}">
            <input
              type="checkbox"
              data-config-key="${escapeHtml(key)}"
              data-field-type="boolean"
              data-field-label="${escapeHtml(label)}"
              ${checked}
              ${disabled}
            >
            <span class="switch-slider"></span>
          </label>
        </div>
      `;
        } else if (type === 'select') {
            const options = (Array.isArray(field.options) ? field.options : []).map((opt) => {
                const selected = String(opt.value) === String(field.value) ? 'selected' : '';
                return `<option value="${escapeHtml(String(opt.value))}" ${selected}>${escapeHtml(String(opt.label || opt.value))}</option>`;
            }).join('');
            const disabled = field.read_only ? 'disabled' : '';
            controlHtml = `
        <select
          data-config-key="${escapeHtml(key)}"
          data-field-type="select"
          data-field-label="${escapeHtml(label)}"
          ${disabled}
        >${options}</select>
      `;
        } else {
            const inputType = type === 'integer' ? 'number' : 'text';
            const disabled = field.read_only ? 'disabled' : '';
            const min = field.min != null ? `min="${field.min}" data-min="${field.min}"` : '';
            const max = field.max != null ? `max="${field.max}" data-max="${field.max}"` : '';
            const step = type === 'integer' ? 'step="1"' : '';
            const value = field.value == null ? '' : String(field.value);
            const placeholder = field.placeholder ? `placeholder="${escapeHtml(String(field.placeholder))}"` : '';
            controlHtml = `
        <input
          type="${inputType}"
          data-config-key="${escapeHtml(key)}"
          data-field-type="${escapeHtml(type)}"
          data-field-label="${escapeHtml(label)}"
          value="${escapeHtml(value)}"
          ${placeholder}
          ${min}
          ${max}
          ${step}
          ${disabled}
        >
      `;
        }

        return `
      <div class="${fieldClasses}">
        <div class="config-field-head">
          <h4 class="config-field-title">${titleHtml}</h4>
          ${chips}
        </div>
        <div class="muted" data-config-description-key="${escapeHtml(key)}" data-base-description="${descriptionHtml}">${descriptionHtml}</div>
        ${controlHtml}
      </div>
    `;
    }

    function formatI2cAddressDescription(baseDescription, key, rawValue) {
        const parsed = Number.parseInt(String(rawValue || '').trim(), 10);
        if (!Number.isFinite(parsed)) {
            return baseDescription;
        }
        if (parsed < 0 || parsed > 127) {
            return `${baseDescription} Current value ${parsed} is outside valid 7-bit range (0-127).`;
        }
        const hex = `0x${parsed.toString(16).toUpperCase().padStart(2, '0')}`;
        if (key === 'display.address') {
            return `${baseDescription} Current: ${parsed} is ${hex}.`;
        }
        if (key === 'led.address') {
            return `${baseDescription} Current: ${parsed} is ${hex}.`;
        }
        return baseDescription;
    }

    function refreshDynamicAddressDescriptions() {
        DYNAMIC_I2C_ADDRESS_KEYS.forEach((key) => {
            const input = document.querySelector(`#configSchemaSections [data-config-key="${key}"]`);
            const descriptionEl = document.querySelector(`#configSchemaSections [data-config-description-key="${key}"]`);
            if (!descriptionEl) return;
            const baseDescription = descriptionEl.dataset.baseDescription || '';
            if (!input) {
                descriptionEl.textContent = baseDescription;
                return;
            }
            descriptionEl.textContent = formatI2cAddressDescription(baseDescription, key, input.value);
        });
    }

    function collectConfigGuiValues() {
        const inputs = document.querySelectorAll('#configSchemaSections [data-config-key]');
        const values = {};
        const errors = [];

        inputs.forEach((input) => {
            if (input.disabled) {
                return;
            }

            const key = input.dataset.configKey;
            const fieldType = input.dataset.fieldType || 'text';
            const label = input.dataset.fieldLabel || key;

            if (!key) {
                return;
            }

            if (fieldType === 'boolean') {
                values[key] = Boolean(input.checked);
                return;
            }

            if (fieldType === 'integer') {
                const parsed = Number(input.value);
                if (!Number.isFinite(parsed) || !Number.isInteger(parsed)) {
                    errors.push(`${label}: enter a whole number.`);
                    return;
                }

                const min = input.dataset.min != null ? Number(input.dataset.min) : null;
                const max = input.dataset.max != null ? Number(input.dataset.max) : null;
                if (min != null && parsed < min) {
                    errors.push(`${label}: minimum is ${min}.`);
                    return;
                }
                if (max != null && parsed > max) {
                    errors.push(`${label}: maximum is ${max}.`);
                    return;
                }

                values[key] = parsed;
                return;
            }

            values[key] = String(input.value);
        });

        return {values, errors};
    }

    async function reloadSystemConfigSchema() {
        const data = await getJson('/api/system/config/schema');
        configSchema = data.schema || {sections: []};
        if (typeof data.raw_toml === 'string') {
            document.getElementById('systemConfigEditor').value = data.raw_toml;
        }
        renderConfigSchema();
        refreshDynamicAddressDescriptions();
        setSystemConfigFeedback('System configuration GUI reloaded from current config.', true);
    }

    async function saveSystemConfigGui() {
        const {values, errors} = collectConfigGuiValues();
        if (errors.length) {
            setSystemConfigFeedback(errors.join(' '), false);
            return;
        }

        try {
            const data = await postJson('/api/system/config/gui', {values});
            configSchema = data.schema || {sections: []};
            if (typeof data.raw_toml === 'string') {
                document.getElementById('systemConfigEditor').value = data.raw_toml;
            }
            renderConfigSchema();
            refreshDynamicAddressDescriptions();
            setSystemConfigFeedback('System configuration saved.', true);
        } catch (err) {
            setSystemConfigFeedback(err.message || 'Failed to save system configuration.', false);
        }
    }

    async function saveSystemConfigRaw() {
        const raw = document.getElementById('systemConfigEditor').value;
        try {
            await postText('/api/system/config/raw', raw);
            await reloadSystemConfigSchema();
            setSystemConfigFeedback('Raw TOML saved and GUI schema refreshed.', true);
        } catch (err) {
            setSystemConfigFeedback(err.message || 'Failed to save raw TOML.', false);
        }
    }

    (function bootConfiguration() {
        renderConfigSchema();
        refreshDynamicAddressDescriptions();
        document.getElementById('configSchemaSections').addEventListener('input', () => {
            refreshDynamicAddressDescriptions();
        });
    })();
