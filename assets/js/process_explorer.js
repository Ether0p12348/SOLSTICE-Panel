    const COLUMN_DEFS = [
        { key: 'name', label: 'Name' },
        { key: 'pid', label: 'PID' },
        { key: 'user', label: 'User' },
        { key: 'cpu', label: 'CPU %' },
        { key: 'mem', label: 'MEM %' },
        { key: 'rss', label: 'RSS' },
        { key: 'vsz', label: 'VSZ' },
        { key: 'state', label: 'State' },
        { key: 'uptime', label: 'Uptime' },
        { key: 'bound', label: 'Bound IPs:Ports' },
        { key: 'command', label: 'Command' },
        { key: 'action', label: 'Action' },
    ];

    let processState = {
        processes: [],
        total: 0,
        current_pid: null,
        available_users: [],
        totals: {},
    };
    let refreshTimer = null;
    let refreshInFlight = false;
    let selectedUser = '';
    const hiddenColumns = new Set();

    function escapeHtml(value) {
        return String(value || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    function setFeedback(message, kind = '') {
        const el = document.getElementById('feedbackLine');
        el.textContent = message || '';
        el.className = 'feedback';
        if (kind) el.classList.add(kind);
    }

    function formatPercent(value) {
        const num = Number(value);
        if (!Number.isFinite(num)) return '-';
        return `${num.toFixed(1)}%`;
    }

    function setSummary(total, shown) {
        const totals = processState.totals || {};
        const base = `${total} processes | CPU Σ ${formatPercent(totals.cpu_percent_sum)} | MEM Σ ${formatPercent(totals.mem_percent_sum)} | RSS Σ ${totals.rss_human_sum || '-'} | VSZ Σ ${totals.vsz_human_sum || '-'}`;
        const withFilter = selectedUser ? `${base} | Showing ${shown}` : base;
        document.getElementById('summaryLine').textContent = withFilter;
        document.getElementById('lastUpdatedLine').textContent = `Updated ${new Date().toLocaleTimeString()}`;
    }

    function applyColumnVisibility() {
        document.querySelectorAll('[data-col]').forEach((cell) => {
            const col = cell.getAttribute('data-col');
            cell.style.display = hiddenColumns.has(col) ? 'none' : '';
        });
    }

    function buildColumnToggles() {
        const container = document.getElementById('columnToggles');
        container.innerHTML = COLUMN_DEFS.map((column) => `
            <label class="column-toggle">
                <input type="checkbox" data-col-toggle="${escapeHtml(column.key)}" checked>
                <span>${escapeHtml(column.label)}</span>
            </label>
        `).join('');

        container.querySelectorAll('input[data-col-toggle]').forEach((input) => {
            input.addEventListener('change', (event) => {
                const target = event.currentTarget;
                const col = target.getAttribute('data-col-toggle');
                if (!col) return;
                if (target.checked) {
                    hiddenColumns.delete(col);
                } else {
                    hiddenColumns.add(col);
                }
                applyColumnVisibility();
            });
        });
    }

    function syncUserFilterOptions() {
        const select = document.getElementById('userFilterSelect');
        const users = Array.isArray(processState.available_users) ? processState.available_users : [];
        const previous = selectedUser;
        const options = ['<option value="">All users</option>']
            .concat(users.map((user) => `<option value="${escapeHtml(user)}">${escapeHtml(user)}</option>`))
            .join('');
        select.innerHTML = options;
        if (previous && users.includes(previous)) {
            select.value = previous;
            selectedUser = previous;
        } else {
            selectedUser = '';
            select.value = '';
        }
    }

    function renderProcesses() {
        const tbody = document.getElementById('processRows');
        const list = Array.isArray(processState.processes) ? processState.processes : [];
        const filtered = selectedUser
            ? list.filter((proc) => String(proc.user || '') === selectedUser)
            : list;

        if (!filtered.length) {
            tbody.innerHTML = `<tr><td colspan="12" class="muted">${
                selectedUser
                    ? `No processes found for user "${escapeHtml(selectedUser)}".`
                    : 'No process data available.'
            }</td></tr>`;
            setSummary(Number(processState.total) || 0, 0);
            applyColumnVisibility();
            return;
        }

        tbody.innerHTML = filtered.map((proc) => {
            const pid = Number(proc.pid);
            const canKill = Boolean(proc.can_kill);
            const protectedReason = String(proc.protected_reason || 'Protected');
            const displayNameJs = JSON.stringify(String(proc.display_name || proc.name || 'process'));
            const endpoints = Array.isArray(proc.bound_endpoints) ? proc.bound_endpoints : [];
            const endpointsHtml = endpoints.length
                ? endpoints.map((ep) => `<div>${escapeHtml(ep)}</div>`).join('')
                : '<span class="muted">-</span>';
            return `
                <tr>
                    <td data-col="name">${escapeHtml(proc.display_name || proc.name)}</td>
                    <td data-col="pid">${escapeHtml(proc.pid)}</td>
                    <td data-col="user">${escapeHtml(proc.user)}</td>
                    <td data-col="cpu">${escapeHtml(proc.cpu_percent)}</td>
                    <td data-col="mem">${escapeHtml(proc.mem_percent)}</td>
                    <td data-col="rss">${escapeHtml(proc.rss_human)}</td>
                    <td data-col="vsz">${escapeHtml(proc.vsz_human)}</td>
                    <td data-col="state">${escapeHtml(proc.state)}</td>
                    <td data-col="uptime">${escapeHtml(proc.elapsed)}</td>
                    <td data-col="bound" class="proc-endpoints">${endpointsHtml}</td>
                    <td data-col="command" class="proc-cmd" title="${escapeHtml(proc.command)}">${escapeHtml(proc.command)}</td>
                    <td data-col="action">
                        ${canKill
                            ? `<button class="danger" type="button" onclick='killProcess(${pid}, ${displayNameJs})'>Kill</button>`
                            : `<span class="muted" title="${escapeHtml(protectedReason)}">Protected</span>`
                        }
                    </td>
                </tr>
            `;
        }).join('');

        setSummary(Number(processState.total) || list.length, filtered.length);
        applyColumnVisibility();
    }

    async function getJson(url) {
        const res = await fetch(url, { cache: 'no-store' });
        const raw = await res.text();
        let data = {};
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

    async function postJson(url) {
        const res = await fetch(url, { method: 'POST' });
        const raw = await res.text();
        let data = {};
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

    async function refreshProcesses() {
        if (refreshInFlight) return;
        refreshInFlight = true;
        try {
            processState = await getJson('/api/processes');
            syncUserFilterOptions();
            renderProcesses();
            setFeedback('', '');
        } catch (err) {
            setFeedback(err.message || 'Failed to load process list.', 'error');
        } finally {
            refreshInFlight = false;
        }
    }

    async function killProcess(pid, displayName) {
        const signal = document.getElementById('signalSelect').value || 'TERM';
        const target = `${displayName || 'process'} (PID ${pid})`;
        const confirmed = window.confirm(`Send SIG${signal} to ${target}?`);
        if (!confirmed) return;

        try {
            await postJson(`/api/processes/${encodeURIComponent(pid)}/kill?signal=${encodeURIComponent(signal)}`);
            setFeedback(`Sent SIG${signal} to PID ${pid}.`, 'ok');
            await refreshProcesses();
        } catch (err) {
            setFeedback(err.message || `Failed to kill PID ${pid}.`, 'error');
        }
    }

    function configureAutoRefresh() {
        const enabled = document.getElementById('autoRefreshToggle').checked;
        if (refreshTimer) {
            clearInterval(refreshTimer);
            refreshTimer = null;
        }
        if (enabled) {
            refreshTimer = setInterval(refreshProcesses, 4000);
        }
    }

    document.getElementById('userFilterSelect').addEventListener('change', (event) => {
        selectedUser = event.target.value || '';
        renderProcesses();
    });
    document.getElementById('autoRefreshToggle').addEventListener('change', configureAutoRefresh);
    buildColumnToggles();
    applyColumnVisibility();
    configureAutoRefresh();
    refreshProcesses();
