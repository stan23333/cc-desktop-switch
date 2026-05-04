  async function renderDashboard() {
    renderProviderCards("#dashboardProviderCards", { includePresets: true })
      .catch((error) => {
        console.error(error);
        const target = $("#dashboardProviderCards");
        if (target) target.innerHTML = `<div class="empty-state">${escapeHtml(error.message || t("toast.requestFailed"))}</div>`;
      });
    Promise.all([
      CCApi.getStatus(),
      CCApi.getActivities(),
    ]).then(([status, activities]) => {
      if (routeFromHash() !== "dashboard") return;
      const health = status.desktopHealth || {};
      const desktopReady = status.desktopConfigured && !health.needsApply;
      const desktopIcon = $("#dashboardDesktopIcon");
      desktopIcon.classList.toggle("muted", !desktopReady);
      desktopIcon.innerHTML = `<i class="bi ${desktopReady ? "bi-check-lg" : "bi-exclamation-lg"}"></i>`;
      const desktopStatus = $("#dashboardDesktopStatus");
      desktopStatus.classList.toggle("muted-text", !desktopReady);
      desktopStatus.textContent = health.needsApply
        ? t("status.needsApply")
        : status.desktopConfigured ? t("status.configured") : t("status.notConfigured");
      renderDesktopHealthWarning("#dashboardDesktopWarning", health);
      $("#dashboardProxyStatus").textContent = status.proxyRunning ? `${t("status.running")} :${status.proxyPort}` : t("status.stopped");
      $("#dashboardProviderName").textContent = status.activeProvider.name;
      $("#activityList").innerHTML = activities.map((item) => (
        `<div class="activity-row"><time>${escapeHtml(item.time)}</time><span>${escapeHtml(item.text)}</span></div>`
      )).join("");
    }).catch((error) => {
      console.error(error);
      if (routeFromHash() === "dashboard") showToast(error.message || t("toast.requestFailed"));
    });
    scheduleUpdateBadgeRefresh();
  }

  async function renderPresets() {
    presetCache = await CCApi.getPresets();
    $("#presetList").innerHTML = presetCache.map((preset) => {
      const active = selectedPreset?.id === preset.id;
      return `
      <button class="preset-item ${active ? "active" : ""}" type="button" data-preset="${escapeHtml(preset.id)}" aria-pressed="${active ? "true" : "false"}">
        <span class="preset-logo">${iconMarkup(preset)}</span>
        <span><strong>${escapeHtml(preset.name)}</strong><span>${escapeHtml(preset.baseUrl)}</span></span>
        <i class="bi ${active ? "bi-check2" : "bi-chevron-right"}"></i>
      </button>
    `;
    }).join("");
  }

  function setProviderFormMode(titleKey) {
    const title = $("#page-providers-add .page-title h1");
    if (title) title.textContent = t(titleKey);
    const submit = $("#providerSaveOnly");
    if (submit) submit.textContent = t("common.saveOnly");
    const result = $("#formSpeedResult");
    if (result) {
      result.textContent = "";
      result.className = "speed-result";
    }
    const modelResult = $("#providerModelFetchResult");
    if (modelResult) modelResult.textContent = "";
  }

  function setApiKeyInputState(hasSavedKey = false, savedKey = "") {
    const input = $("#providerApiKey");
    const label = $("label[for='providerApiKey']");
    if (!input) return;
    input.type = "password";
    input.value = savedKey || "";
    input.required = !hasSavedKey && !savedKey;
    input.placeholder = (hasSavedKey || savedKey) ? t("providers.keySavedPlaceholder") : t("providers.keyPlaceholder");
    const toggle = $("[data-action='toggle-key']");
    if (toggle) toggle.innerHTML = '<i class="bi bi-eye"></i>';
    if (label) label.classList.toggle("required", input.required);
  }

  function updateDetectFormatButton() {
    const btn = $("#detectFormatBtn");
    const status = $("#detectFormatStatus");
    if (!btn) return;
    const baseUrl = $("#providerBaseUrl")?.value.trim();
    const apiKey = $("#providerApiKey")?.value.trim();
    const canDetect = !!(baseUrl && apiKey);
    btn.disabled = !canDetect;
    if (status && !status.textContent) {
      status.className = "detect-format-status";
    }
  }

  function updateApplyButtonState() {
    const applyBtn = $("[data-action='apply-provider-desktop']");
    if (!applyBtn) return;
    const isThirdParty = selectedPreset?.id === "third-party";
    const canApply = !isThirdParty || protocolDetected;
    applyBtn.disabled = !canApply;
  }

  async function handleDetectFormat() {
    const btn = $("#detectFormatBtn");
    const status = $("#detectFormatStatus");
    if (!btn || !status) return;

    const baseUrl = $("#providerBaseUrl")?.value.trim();
    const apiKey = $("#providerApiKey")?.value.trim();
    if (!baseUrl || !apiKey) return;

    btn.disabled = true;
    status.textContent = "探测中...";
    status.className = "detect-format-status";

    try {
      const result = await CCApi.detectApiFormat(baseUrl, apiKey);
      if (result.success) {
        const fmt = result.apiFormat === "openai_responses" ? "openai_chat" : result.apiFormat;
        setFormApiFormat(fmt);
        const fmtLabel = fmt === "anthropic" ? "Anthropic 兼容" : fmt === "openai_chat" ? "OpenAI Chat" : "OpenAI Responses";
        status.textContent = `已识别: ${fmtLabel}`;
        status.className = "detect-format-status success";
        protocolDetected = true;
        updateApplyButtonState();
      } else {
        status.textContent = result.message || "探测失败";
        status.className = "detect-format-status error";
      }
    } catch (error) {
      status.textContent = error.message || "探测失败";
      status.className = "detect-format-status error";
    } finally {
      updateDetectFormatButton();
    }
  }

  function resetProviderForm() {
    editingProviderId = null;
    selectedPreset = null;
    providerAvailableModels = [];
    baseUrlMenuOpen = false;
    renderPresetOptions(null);
    updatePresetSelection();
    formModelCapabilities = {};
    formRequestOptions = {};
    protocolDetected = false;
    setProviderFormMode("providersAdd.title");
    $("#providerName").value = "";
    $("#providerBaseUrl").value = "";
    renderBaseUrlOptions(null);
    setApiKeyInputState(false);
    $("#providerAuth").value = "bearer";
    renderAuthSchemeControl();
    setFormApiFormat("anthropic");
    setProviderMappings(emptyMappings());
    updateDetectFormatButton();
    updateApplyButtonState();
  }

  function applyPresetToForm(preset, notify = true) {
    $("#providerName").value = preset.name;
    $("#providerBaseUrl").value = preset.baseUrl;
    baseUrlMenuOpen = false;
    renderBaseUrlOptions(preset);
    setAuthSchemeValue(preset.authScheme);
    setApiKeyInputState(false);
    selectedPreset = preset;
    setFormApiFormat(preset.apiFormat === "OpenAI" ? "openai_chat" : "anthropic");
    formModelCapabilities = normalizeCapabilities(preset.modelCapabilities || {});
    formRequestOptions = normalizeRequestOptions(preset.requestOptions || {});
    providerAvailableModels = [];
    setProviderMappings(preset.models || emptyMappings());
    renderPresetOptions(preset, preset.models || emptyMappings());
    updatePresetSelection();
    // 内置预设已知协议，第三方需要探测
    protocolDetected = preset.id !== "third-party";
    updateDetectFormatButton();
    updateApplyButtonState();
    if (notify) showToast(`${preset.name} ${t("toast.presetFilled")}`);
  }

  async function fillProviderForEdit(providerId) {
    const providers = await CCApi.getProviders();
    const provider = providers.find((item) => item.id === providerId);
    if (!provider) return;
    editingProviderId = provider.id;
    const matchedPreset = presetCache.find((preset) => presetMatchesProvider(preset, provider));
    selectedPreset = matchedPreset
      ? { ...matchedPreset, extraHeaders: provider.extraHeaders || matchedPreset.extraHeaders || {} }
      : {
        models: provider.mappings,
        extraHeaders: provider.extraHeaders || {},
        modelCapabilities: provider.modelCapabilities || {},
        requestOptions: provider.requestOptions || {},
      };
    formModelCapabilities = normalizeCapabilities(provider.modelCapabilities || selectedPreset.modelCapabilities || {});
    formRequestOptions = normalizeRequestOptions(provider.requestOptions || selectedPreset.requestOptions || {});
    setProviderFormMode("providersAdd.editTitle");
    $("#providerName").value = provider.name;
    $("#providerBaseUrl").value = provider.baseUrl;
    baseUrlMenuOpen = false;
    renderBaseUrlOptions(selectedPreset);
    setApiKeyInputState(provider.hasApiKey);
    if (provider.hasApiKey) {
      try {
        const secret = await CCApi.getProviderSecret(provider.id);
        setApiKeyInputState(true, secret.apiKey || "");
      } catch (error) {
        console.error(error);
        showToast(error.message || t("toast.requestFailed"));
      }
    }
    setAuthSchemeValue(provider.authScheme);
    setFormApiFormat(["openai", "openai_chat"].includes(provider.apiFormat) ? "openai_chat" : "anthropic");
    providerAvailableModels = [];
    setProviderMappings(provider.mappings || emptyMappings());
    renderPresetOptions(selectedPreset, provider.mappings || emptyMappings());
    updatePresetSelection();
    protocolDetected = true;
    updateDetectFormatButton();
    updateApplyButtonState();
  }

  async function renderProviderForm() {
    await renderPresets();
    if (editingProviderId) {
      await fillProviderForEdit(editingProviderId);
      return;
    }
    if (selectedPreset) {
      setProviderFormMode("providersAdd.title");
      applyPresetToForm(selectedPreset, false);
      return;
    }
    resetProviderForm();
  }

  async function renderProviders() {
    await renderModelMenuModePanel();
    await renderProviderCards("#providerRows");
  }

  function renderModelMenuModeState(settings = {}) {
    const enabled = !!settings.exposeAllProviderModels;
    const button = $("#modelMenuModeToggle");
    const hint = $("#modelMenuModeHint");
    if (button) {
      button.classList.toggle("btn-primary", enabled);
      button.classList.toggle("btn-outline-primary", !enabled);
      const span = $("span", button);
      if (span) span.textContent = enabled ? t("providers.showSingleModel") : t("providers.showAllModels");
      button.setAttribute("aria-pressed", enabled ? "true" : "false");
    }
    if (hint) {
      hint.textContent = enabled ? t("providers.modelMenuAllHint") : t("providers.modelMenuSingleHint");
    }
    const settingToggle = $("#exposeAllProviderModels");
    if (settingToggle) settingToggle.checked = enabled;
  }

  async function renderModelMenuModePanel() {
    const settings = await CCApi.getSettings();
    renderModelMenuModeState(settings);
  }

  async function renderModelSelectors() {
    const providers = await CCApi.getProviders();
    const select = $("#modelProvider");
    select.innerHTML = providers.map((provider) => `<option value="${escapeHtml(provider.id)}">${escapeHtml(provider.name)}</option>`).join("");
    const active = providers.find((provider) => provider.default) || providers[0];
    if (active) select.value = active.id;
    renderMappingCards();
  }

  async function renderMappingCards() {
    const providers = await CCApi.getProviders();
    const provider = providers.find((item) => item.id === $("#modelProvider").value) || providers[0];
    if (!provider) return;
    const defaultSelect = $("#defaultModel");
    if (defaultSelect) {
      const defaultValue = provider.mappings.default || provider.mappings.sonnet_4_6 || "";
      const defaultKey = modelMeta.find((model) => provider.mappings[model.key] === defaultValue)?.key || "sonnet";
      defaultSelect.value = defaultKey;
    }
    const result = $("#modelFetchResult");
    if (result) result.textContent = "";
    $("#mappingStack").innerHTML = modelMeta.map((model) => `
      <article class="mapping-card">
        <div class="mapping-title">
          <span class="mapping-icon ${model.key}"><i class="bi ${model.icon}"></i></span>
          <strong>${model.title}</strong>
          <span class="alias-pill">${model.title}</span>
        </div>
        <input class="form-control form-control-lg" data-model-input="${model.key}" value="${escapeHtml(provider.mappings[model.key] || "")}">
        <span class="source-model"><i class="bi bi-arrow-left"></i>${model.source}</span>
      </article>
    `).join("");
  }

  async function renderDesktop() {
    const desktop = await CCApi.getDesktopStatus();
    const entries = Object.entries(desktop.config);
    const health = desktop.health || {};
    const desktopReady = desktop.configured && !health.needsApply;
    const statusText = $("#desktopConfiguredText");
    statusText.textContent = health.needsApply
      ? t("status.needsApply")
      : desktop.configured ? t("status.configured") : t("status.notConfigured");
    statusText.classList.toggle("muted-text", !desktopReady);
    $(".desktop-card .circle-check")?.classList.toggle("warning", !desktopReady);
    renderDesktopHealthWarning("#desktopPageWarning", health);
    $("#desktopConfigList").innerHTML = entries.map(([key, value]) => `
      <div class="config-row"><i class="bi bi-check-circle-fill"></i><span>${escapeHtml(key)}:</span><code>${escapeHtml(Array.isArray(value) ? JSON.stringify(value) : value)}</code></div>
    `).join("");
    $("#desktopJson").textContent = JSON.stringify(desktop.config, null, 2);
  }

  async function refreshProxyLog() {
    if (proxyLogInflight) return;
    const logEl = $("#proxyLog");
    if (!logEl) return;
    proxyLogInflight = true;
    try {
      const [proxyStatus, logs] = await Promise.all([
        CCApi.getProxyStatus(),
        CCApi.getProxyLogs(),
      ]);
      logEl.innerHTML = logs.map((line) => `
        <div class="log-line"><span>${escapeHtml(line.at)}</span><span class="log-level ${escapeHtml(line.level)}">${escapeHtml(line.level.toUpperCase())}</span><span>${escapeHtml(line.message)}</span></div>
      `).join("");
      if ($("#autoScroll")?.checked) logEl.scrollTop = logEl.scrollHeight;
      const stats = [
        { label: t("proxy.stats.total"), value: proxyStatus.stats.total, icon: "bi-list-ul" },
        { label: t("proxy.stats.success"), value: proxyStatus.stats.success, icon: "bi-check-circle" },
        { label: t("proxy.stats.failed"), value: proxyStatus.stats.failed, icon: "bi-x-circle", danger: true },
        { label: t("proxy.stats.today"), value: proxyStatus.stats.today, icon: "bi-calendar3" },
      ];
      $("#proxyStats").innerHTML = stats.map((stat) => `
        <article class="stat-card ${stat.danger ? "danger" : ""}"><i class="bi ${stat.icon}"></i><div><span>${stat.label}</span><strong>${stat.value}</strong></div></article>
      `).join("");
    } catch (error) {
      console.warn(error);
    } finally {
      proxyLogInflight = false;
    }
  }

  function stopProxyLogAutoRefresh() {
    if (proxyLogTimer !== null) {
      clearInterval(proxyLogTimer);
      proxyLogTimer = null;
    }
  }

  function startProxyLogAutoRefresh() {
    stopProxyLogAutoRefresh();
    proxyLogTimer = setInterval(() => {
      if (document.visibilityState === "hidden") return;
      refreshProxyLog();
    }, 2000);
  }

  async function renderProxy() {
    const status = await CCApi.getStatus();
    $("#proxyPort").value = status.proxyPort;
    $("#settingsProxyPort").value = status.proxyPort;
    $("#proxyStateText").textContent = status.proxyRunning ? t("status.running") : t("status.stopped");
    await refreshProxyLog();
    if (routeFromHash() === "proxy") {
      startProxyLogAutoRefresh();
    } else {
      stopProxyLogAutoRefresh();
    }
  }

  async function renderSettings() {
    const settings = await CCApi.getSettings();
    applyTheme(settings.theme || "default");
    $("#settingsProxyPort").value = settings.proxyPort;
    $("#settingsAdminPort").value = settings.adminPort;
    $("#autoStart").checked = settings.autoStart;
    $("#exposeAllProviderModels").checked = !!settings.exposeAllProviderModels;
    $("#settingsUpdateUrl").value = settings.updateUrl || "";
    $("#settingsUpstreamProxy").value = settings.upstreamProxy || "";
    $("#settingsUpstreamProxyEnabled").checked = settings.upstreamProxyEnabled !== false;
    renderModelMenuModeState(settings);
    await refreshBackupList();
    await refreshCcSwitchImportStatus();
  }

  async function renderRoute(route) {
    $all(".page").forEach((page) => page.classList.toggle("active", page.dataset.page === route));
    $all(".route-tab").forEach((tab) => {
      const key = route.startsWith("providers") ? "providers" : route;
      tab.classList.toggle("active", tab.dataset.nav === key);
    });
    if (route !== "proxy") stopProxyLogAutoRefresh();
    if (route === "dashboard") await renderDashboard();
    if (route === "providers/add") await renderProviderForm();
    if (route === "providers") await renderProviders();
    if (route === "desktop") await renderDesktop();
    if (route === "proxy") await renderProxy();
    if (route === "settings") await renderSettings();
  }
