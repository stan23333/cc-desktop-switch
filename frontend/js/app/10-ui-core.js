  function $(selector, root = document) {
    return root.querySelector(selector);
  }

  function $all(selector, root = document) {
    return Array.from(root.querySelectorAll(selector));
  }

  function routeFromHash() {
    const hash = window.location.hash.replace(/^#/, "");
    return routes.includes(hash) ? hash : "dashboard";
  }

  function showToast(message) {
    $("#toastBody").textContent = message;
    toast.show();
  }

  function confirmAction(message) {
    const modalEl = $("#confirmModal");
    const body = $("#confirmModalBody");
    const confirmBtn = $("#confirmModalConfirm");
    if (!modalEl || !body || !confirmBtn || !confirmModal) {
      return Promise.resolve(window.confirm(message));
    }

    body.textContent = message;
    confirmBtn.textContent = t("common.confirm");

    return new Promise((resolve) => {
      let settled = false;
      const finish = (value) => {
        if (settled) return;
        settled = true;
        modalEl.removeEventListener("hidden.bs.modal", handleHidden);
        confirmBtn.removeEventListener("click", handleConfirm);
        resolve(value);
      };
      const handleHidden = () => finish(false);
      const handleConfirm = () => {
        finish(true);
        confirmModal.hide();
      };

      modalEl.addEventListener("hidden.bs.modal", handleHidden);
      confirmBtn.addEventListener("click", handleConfirm);
      confirmModal.show();
    });
  }

  let _mappingAlertTimer = null;
  function showMappingCheckAlert(model, available, message) {
    const wrap = $("#mappingCheckAlertWrap");
    if (!wrap) return;
    const alertClass = available ? "alert-success" : "alert-danger";
    const iconClass = available ? "bi-check-circle-fill" : "bi-x-circle-fill";
    const title = available ? t("toast.modelAvailable") : t("toast.modelUnavailable");
    wrap.innerHTML = `
      <div class="alert ${alertClass} mapping-check-alert alert-dismissible fade show" role="alert">
        <i class="bi ${iconClass}"></i>
        <strong>${escapeHtml(model)}</strong> ${escapeHtml(title)}：${escapeHtml(message)}
        <button type="button" class="btn-close" data-bs-dismiss="alert" aria-label="Close"></button>
      </div>
    `;
    if (_mappingAlertTimer) clearTimeout(_mappingAlertTimer);
    _mappingAlertTimer = setTimeout(() => {
      wrap.innerHTML = "";
    }, 5000);
  }

  function restartReminderDismissed() {
    try {
      return localStorage.getItem(restartReminderStorageKey) === "1";
    } catch (error) {
      return false;
    }
  }

  function showRestartReminder() {
    if (restartReminderDismissed()) return;
    const checkbox = $("#restartReminderDontShow");
    if (checkbox) checkbox.checked = false;
    restartReminderModal?.show();
  }

  function dismissRestartReminder() {
    const checkbox = $("#restartReminderDontShow");
    if (checkbox?.checked) {
      try {
        localStorage.setItem(restartReminderStorageKey, "1");
      } catch (error) {
        console.warn(error);
      }
    }
    restartReminderModal?.hide();
  }

  async function restartClaudeDesktopFromUi(button, hideReminder = false) {
    const originalHtml = button?.innerHTML;
    if (button) {
      button.disabled = true;
      button.innerHTML = `<span class="spinner-border spinner-border-sm" aria-hidden="true"></span><span>${escapeHtml(t("restartReminder.restarting"))}</span>`;
    }
    try {
      const result = await CCApi.restartClaudeDesktop();
      showToast(result.message || t("toast.claudeRestartRequested"));
      if (hideReminder) dismissRestartReminder();
    } finally {
      if (button) {
        button.disabled = false;
        button.innerHTML = originalHtml;
      }
    }
  }

  async function confirmRestartClaudeAfterEnable() {
    if (!(await confirmAction(t("confirm.restartClaudeAfterEnable")))) return;
    await restartClaudeDesktopFromUi(null);
  }

  function t(key) {
    return CCI18n.t(key);
  }

  function formatI18n(key, values = {}) {
    return t(key).replace(/\{(\w+)\}/g, (_, name) => (
      Object.prototype.hasOwnProperty.call(values, name) ? values[name] : `{${name}}`
    ));
  }

  function iconMarkup(item) {
    if (item.logo) return `<img src="${item.logo}" alt="">`;
    if (item.iconText) return `<span>${item.iconText}</span>`;
    return `<i class="bi ${item.icon || "bi-plug-fill"}"></i>`;
  }

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (char) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "\"": "&quot;",
      "'": "&#39;",
    }[char]));
  }

  function safeHttpUrl(value) {
    try {
      const parsed = new URL(String(value || ""), window.location.origin);
      if (["http:", "https:"].includes(parsed.protocol)) return parsed.href;
    } catch (error) {
      return "#";
    }
    return "#";
  }

  function normalizePresetKey(value) {
    return String(value || "").trim().toLowerCase().replace(/\/+$/, "");
  }

  function presetExists(preset, providers) {
    const presetName = normalizePresetKey(preset.name);
    const presetUrl = normalizePresetKey(preset.baseUrl);
    return providers.some((provider) => (
      normalizePresetKey(provider.name) === presetName
      || normalizePresetKey(provider.baseUrl) === presetUrl
    ));
  }

  function updatePresetSelection() {
    const selectedId = selectedPreset?.id || "";
    $all("#presetList [data-preset]").forEach((button) => {
      const active = button.dataset.preset === selectedId;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", active ? "true" : "false");
      const icon = $("i:last-child", button);
      if (icon) icon.className = `bi ${active ? "bi-check2" : "bi-chevron-right"}`;
    });
  }

  function setFormApiFormat(format) {
    formApiFormat = ["OpenAI", "openai", "openai_chat"].includes(format) ? "OpenAI" : "Anthropic";
    const activeFormat = formApiFormat === "OpenAI" ? "openai_chat" : "anthropic";
    $all("[data-api-format]").forEach((button) => {
      const active = button.dataset.apiFormat === activeFormat;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", active ? "true" : "false");
    });
  }

  function firstHealthMessage(health) {
    return health?.issues?.[0]?.message || "";
  }

  function renderDesktopHealthWarning(selector, health) {
    const warning = $(selector);
    if (!warning) return;
    const message = firstHealthMessage(health);
    warning.hidden = !message;
    const text = $("span", warning);
    if (text) text.textContent = message;
  }

  function renderUpdateBadge(result) {
    const badge = $("#dashboardUpdateBadge");
    const available = !!result?.updateAvailable;
    const installButton = $("#settingsInstallUpdate");
    const busy = updateInstallPhase !== "idle";
    if (badge) {
      badge.hidden = !(available || busy);
      badge.disabled = busy;
      badge.title = available && !busy ? t("settings.installUpdate") : "";
      badge.setAttribute("aria-label", available ? t("settings.installUpdate") : t("dashboard.updateAvailable"));
    }
    if (installButton) {
      installButton.hidden = !(available || busy);
      installButton.disabled = busy;
    }
    const badgeIcon = badge ? $("i", badge) : null;
    if (badgeIcon) {
      badgeIcon.className = busy ? "bi bi-arrow-repeat" : "bi bi-cloud-arrow-down";
    }
    const installIcon = installButton ? $("i", installButton) : null;
    if (installIcon) {
      installIcon.className = busy ? "bi bi-arrow-repeat" : "bi bi-download";
    }
    const text = badge ? $("span", badge) : null;
    if (text) {
      if (updateInstallPhase === "downloading") {
        text.textContent = t("settings.downloadingUpdate");
      } else if (updateInstallPhase === "installing") {
        text.textContent = t("settings.installingUpdate");
      } else if (available) {
        text.textContent = result.latestVersion
          ? `${t("dashboard.updateAvailable")} ${result.latestVersion}`
          : t("dashboard.updateAvailable");
      }
    }
    const installText = installButton ? $("span", installButton) : null;
    if (installText) {
      if (updateInstallPhase === "downloading") {
        installText.textContent = t("settings.downloadingUpdate");
      } else if (updateInstallPhase === "installing") {
        installText.textContent = t("settings.installingUpdate");
      } else {
        installText.textContent = t("settings.installUpdate");
      }
    }
  }

  function setUpdateInstallPhase(phase = "idle") {
    updateInstallPhase = phase;
    renderUpdateBadge(updateCheckCache);
  }

  async function refreshUpdateBadge(force = false) {
    try {
      updateCheckCache = await CCApi.checkUpdate("");
      renderUpdateBadge(updateCheckCache);
    } catch (error) {
      console.warn(error);
      updateCheckCache = null;
      renderUpdateBadge(null);
    }
  }

  function scheduleUpdateBadgeRefresh() {
    window.requestAnimationFrame(() => {
      window.setTimeout(() => refreshUpdateBadge(), 1000);
    });
  }

  function focusCcSwitchImportSection() {
    const section = $("#ccSwitchImportSection");
    if (!section) return;
    section.scrollIntoView({ behavior: "smooth", block: "center" });
    section.classList.add("focus-flash");
    window.setTimeout(() => section.classList.remove("focus-flash"), 1400);
  }

  function openCcSwitchImportSettings() {
    if (routeFromHash() !== "settings") {
      window.location.hash = "settings";
      window.setTimeout(focusCcSwitchImportSection, 260);
      return;
    }
    focusCcSwitchImportSection();
  }

  function emptyMappings() {
    return Object.fromEntries(providerFormModelSlots.map((slot) => [slot.key, ""]));
  }

  function normalizeMappings(mappings = {}) {
    const normalized = emptyMappings();
    if (!mappings || typeof mappings !== "object") return normalized;
    normalized.default = String(mappings.default || "").trim();
    normalized.opus_4_7 = String(mappings.opus_4_7 || mappings.opus || "").trim();
    normalized.opus_4_6 = String(mappings.opus_4_6 || "").trim();
    normalized.opus_3 = String(mappings.opus_3 || "").trim();
    normalized.sonnet_4_6 = String(mappings.sonnet_4_6 || mappings.sonnet || "").trim();
    normalized.sonnet_4_5 = String(mappings.sonnet_4_5 || "").trim();
    normalized.haiku_4_5 = String(mappings.haiku_4_5 || mappings.haiku || "").trim();
    return normalized;
  }

  function normalizeCapabilities(capabilities = {}) {
    if (!capabilities || typeof capabilities !== "object") return {};
    return Object.fromEntries(Object.entries(capabilities).filter(([, value]) => (
      value && typeof value === "object" && value.supports1m === true
    )));
  }

  function normalizeRequestOptions(options = {}) {
    if (!options || typeof options !== "object") return {};
    const source = options.anthropic && typeof options.anthropic === "object"
      ? options.anthropic
      : options;
    const normalized = {};
    const thinkingType = source.thinking?.type;
    if (["enabled", "disabled"].includes(thinkingType)) {
      normalized.thinking = { type: thinkingType };
    }
    const effort = source.output_config?.effort;
    if (["low", "medium", "high", "xhigh", "max"].includes(effort)) {
      normalized.output_config = { effort };
    }
    return Object.keys(normalized).length ? { anthropic: normalized } : {};
  }

  function mergeRequestOptions(base = {}, extra = {}) {
    const baseAnthropic = normalizeRequestOptions(base).anthropic || {};
    const extraAnthropic = normalizeRequestOptions(extra).anthropic || {};
    const merged = { ...baseAnthropic };
    if (extraAnthropic.thinking) merged.thinking = { ...extraAnthropic.thinking };
    if (extraAnthropic.output_config) merged.output_config = { ...extraAnthropic.output_config };
    return normalizeRequestOptions({ anthropic: merged });
  }

  function clearRequestOptions(base = {}, option = {}) {
    const baseAnthropic = normalizeRequestOptions(base).anthropic || {};
    const optionAnthropic = normalizeRequestOptions(option).anthropic || {};
    const current = { ...baseAnthropic };
    if (optionAnthropic.thinking) delete current.thinking;
    if (optionAnthropic.output_config) delete current.output_config;
    return normalizeRequestOptions({ anthropic: current });
  }

  function requestOptionsMatch(left = {}, right = {}) {
    return JSON.stringify(normalizeRequestOptions(left)) === JSON.stringify(normalizeRequestOptions(right));
  }

  function capabilitiesMatch(left = {}, right = {}) {
    return JSON.stringify(normalizeCapabilities(left)) === JSON.stringify(normalizeCapabilities(right));
  }

  function mergeCapabilities(base = {}, extra = {}) {
    return {
      ...normalizeCapabilities(base),
      ...normalizeCapabilities(extra),
    };
  }

  function clearCapabilities(base = {}, option = {}) {
    const current = normalizeCapabilities(base);
    Object.keys(normalizeCapabilities(option)).forEach((modelId) => {
      delete current[modelId];
    });
    return current;
  }

  function optionEnabled(option = {}, currentMappings = collectProviderMappings()) {
    const hasModels = option.models && typeof option.models === "object";
    const hasRequestOptions = option.requestOptions && typeof option.requestOptions === "object";
    const hasCapabilities = option.modelCapabilities && typeof option.modelCapabilities === "object";
    const modelsOk = !hasModels || modelsMatch(option.models, currentMappings);
    const requestOptionsOk = !hasRequestOptions || requestOptionsMatch(option.requestOptions, formRequestOptions);
    const optionChangesModels = hasModels && !modelsMatch(option.models, selectedPreset?.models || {});
    const capabilitiesOk = !hasCapabilities || optionChangesModels || capabilitiesMatch(option.modelCapabilities, formModelCapabilities);
    if (hasModels || hasRequestOptions || hasCapabilities) {
      return modelsOk && requestOptionsOk && capabilitiesOk;
    }
    return false;
  }

  function modelsMatch(left = {}, right = {}) {
    const a = normalizeMappings(left);
    const b = normalizeMappings(right);
    return providerFormModelSlots.every((slot) => (a[slot.key] || "") === (b[slot.key] || ""));
  }

  function presetMatchesProvider(preset, provider) {
    if (!preset || !provider) return false;
    const baseUrlOptions = Array.isArray(preset.baseUrlOptions) ? preset.baseUrlOptions : [];
    return normalizePresetKey(preset.name) === normalizePresetKey(provider.name)
      || normalizePresetKey(preset.baseUrl) === normalizePresetKey(provider.baseUrl)
      || baseUrlOptions.some((option) => normalizePresetKey(option?.value) === normalizePresetKey(provider.baseUrl));
  }

  function presetBaseUrlOptions(preset = null) {
    return Array.isArray(preset?.baseUrlOptions) ? preset.baseUrlOptions.filter((option) => option?.value) : [];
  }

  function closeBaseUrlMenu() {
    if (!baseUrlMenuOpen) return;
    baseUrlMenuOpen = false;
    renderBaseUrlOptions();
  }

  function toggleBaseUrlMenu() {
    if (!presetBaseUrlOptions(selectedPreset).length) return;
    baseUrlMenuOpen = !baseUrlMenuOpen;
    renderBaseUrlOptions();
  }

  function setBaseUrlValue(value) {
    const input = $("#providerBaseUrl");
    if (!input) return;
    input.value = value;
    closeBaseUrlMenu();
  }

  function renderBaseUrlOptions(preset = selectedPreset) {
    const input = $("#providerBaseUrl");
    const trigger = $("#providerBaseUrlTrigger");
    const menu = $("#providerBaseUrlMenu");
    const wrap = $("#providerBaseUrlControl");
    const hint = $("#providerBaseUrlHint");
    if (!input || !trigger || !menu || !wrap || !hint) return;
    const options = presetBaseUrlOptions(preset);
    const helpText = String(preset?.baseUrlHint || "").trim();
    trigger.hidden = !options.length;
    trigger.disabled = !options.length;
    trigger.setAttribute("aria-expanded", options.length && baseUrlMenuOpen ? "true" : "false");
    wrap.classList.toggle("open", !!options.length && baseUrlMenuOpen);
    menu.innerHTML = options.map((option) => {
      const selected = input.value.trim() === option.value;
      return `
        <button
          class="baseurl-option ${selected ? "selected" : ""}"
          type="button"
          role="option"
          data-action="select-baseurl-option"
          data-baseurl-value="${escapeHtml(option.value)}"
          aria-selected="${selected ? "true" : "false"}"
        >
          <span>${escapeHtml(option.value)}</span>
          <small>${escapeHtml(option.label || "")}</small>
          ${selected ? '<i class="bi bi-check2"></i>' : ""}
        </button>
      `;
    }).join("");
    hint.textContent = helpText;
    hint.hidden = !helpText;
  }

  function capabilitiesForCurrentMappings(mappings = collectProviderMappings()) {
    const usedModelIds = new Set(Object.values(mappings).filter(Boolean));
    return Object.fromEntries(Object.entries(normalizeCapabilities(formModelCapabilities)).filter(([modelId]) => (
      usedModelIds.has(modelId)
    )));
  }

  function formMappingRowsFromMappings(mappings = {}) {
    const rows = [...providerFormDefaultRows];
    providerFormModelSlots.forEach((slot) => {
      if (slot.key !== "default" && mappings[slot.key] && !rows.includes(slot.key)) {
        rows.push(slot.key);
      }
    });
    return rows;
  }

  function slotByKey(key) {
    return providerFormModelSlots.find((slot) => slot.key === key) || providerFormModelSlots[0];
  }

  function slotOptionsForRow(currentKey) {
    const used = new Set(providerFormRows.filter((key) => key !== currentKey));
    return providerFormModelSlots.filter((slot) => !used.has(slot.key));
  }

  function providerModelOptionsMarkup(currentValue = "") {
    return providerAvailableModels.map((modelId) => (`
      <button
        class="mapping-slot-option ${modelId === currentValue ? "selected" : ""}"
        type="button"
        role="option"
        data-action="select-provider-model-option"
        data-model-value="${escapeHtml(modelId)}"
        aria-selected="${modelId === currentValue ? "true" : "false"}"
      >
        <span>${escapeHtml(modelId)}</span>
        ${modelId === currentValue ? '<i class="bi bi-check2"></i>' : ""}
      </button>
    `)).join("");
  }

  function slotMenuMarkup(rowKey, index) {
    const slot = slotByKey(rowKey);
    const isRequired = rowKey === "default";
    const expanded = openProviderSlotMenuIndex === index;
    const options = slotOptionsForRow(rowKey).map((option) => (`
      <button
        class="mapping-slot-option ${option.key === rowKey ? "selected" : ""}"
        type="button"
        role="option"
        data-action="select-provider-model-slot"
        data-row-index="${index}"
        data-slot-key="${escapeHtml(option.key)}"
        aria-selected="${option.key === rowKey ? "true" : "false"}"
      >
        <span>${escapeHtml(option.label)}</span>
        ${option.key === rowKey ? '<i class="bi bi-check2"></i>' : ""}
      </button>
    `)).join("");
    return `
      <div class="mapping-slot-menu-wrap ${expanded ? "open" : ""}">
        <button
          class="form-select mapping-slot-trigger"
          id="providerMappingSlot-${index}"
          type="button"
          ${isRequired ? "disabled" : ""}
          data-action="toggle-provider-model-slot-menu"
          data-row-index="${index}"
          aria-haspopup="listbox"
          aria-expanded="${expanded ? "true" : "false"}"
        >
          <span>${escapeHtml(slot.label)}</span>
          <i class="bi bi-chevron-down"></i>
        </button>
        ${isRequired ? "" : `
          <div class="mapping-slot-menu" role="listbox" aria-labelledby="providerMappingSlot-${index}">
            ${options}
          </div>
        `}
      </div>
    `;
  }

  function formMappingMarkup() {
    return providerFormRows.map((rowKey, index) => {
      const slot = slotByKey(rowKey);
      const isRequired = rowKey === "default";
      const currentProviderModel = providerFormMappings[rowKey] || "";
      const canCheck = !!currentProviderModel && !!editingProviderId;
      return `
        <article class="form-mapping-row">
          <div class="form-mapping-left">
            <label class="form-label visually-hidden" for="providerMappingSlot-${index}">${t("providersAdd.claudeModel")}</label>
            <div class="mapping-select-wrap">
              <span class="mapping-icon ${slot.iconClass}"><i class="bi ${slot.icon}"></i></span>
              ${slotMenuMarkup(rowKey, index)}
            </div>
          </div>
          <div class="form-mapping-right">
            <label class="form-label visually-hidden" for="providerMappingValue-${index}">${t("providersAdd.providerModel")}</label>
            <div class="provider-model-input-wrap ${openProviderModelMenuKey === rowKey ? "open" : ""}">
              <input
                class="form-control provider-model-input"
                id="providerMappingValue-${index}"
                data-provider-model-input="${escapeHtml(rowKey)}"
                value="${escapeHtml(providerFormMappings[rowKey] || "")}"
                placeholder="${escapeHtml(t("providersAdd.providerModelPlaceholder"))}"
                ${isRequired ? "required" : ""}
              >
              <button
                class="provider-model-trigger"
                type="button"
                data-action="toggle-provider-model-menu"
                data-row-key="${escapeHtml(rowKey)}"
                ${providerAvailableModels.length ? "" : "disabled"}
                aria-haspopup="listbox"
                aria-expanded="${providerAvailableModels.length && openProviderModelMenuKey === rowKey ? "true" : "false"}"
                aria-label="${escapeHtml(t("providersAdd.providerModel"))}"
              >
                <i class="bi bi-chevron-down" aria-hidden="true"></i>
              </button>
              ${providerAvailableModels.length ? `
                <div class="mapping-slot-menu provider-model-menu" role="listbox" aria-labelledby="providerMappingValue-${index}">
                  ${providerModelOptionsMarkup(currentProviderModel)}
                </div>
              ` : ""}
            </div>
          </div>
          <div class="form-mapping-actions">
            ${canCheck
              ? `<button class="btn btn-outline-primary btn-sm mapping-check-button" type="button" data-action="check-model" data-row-key="${escapeHtml(rowKey)}" aria-label="${escapeHtml(t("providersAdd.checkModel"))}">${escapeHtml(t("providersAdd.checkModel"))}</button>`
              : '<span class="mapping-check-placeholder" aria-hidden="true"></span>'}
            ${isRequired
              ? '<span class="mapping-remove-placeholder" aria-hidden="true"></span>'
              : `<button class="btn btn-outline-danger btn-sm mapping-remove-button" type="button" data-action="remove-provider-model-row" data-row-index="${index}" aria-label="${escapeHtml(t("providersAdd.removeMapping"))}">${escapeHtml(t("providersAdd.removeMapping"))}</button>`}
          </div>
        </article>
      `;
    }).join("");
  }

  function renderProviderMappings() {
    const stack = $("#providerMappingStack");
    if (!stack) return;
    if (openProviderSlotMenuIndex !== null && !providerFormRows[openProviderSlotMenuIndex]) {
      openProviderSlotMenuIndex = null;
    }
    if (openProviderModelMenuKey !== null && !providerFormRows.includes(openProviderModelMenuKey)) {
      openProviderModelMenuKey = null;
    }
    const canAddMoreRows = providerFormModelSlots.some((slot) => !providerFormRows.includes(slot.key));
    stack.innerHTML = `
      <div class="provider-mapping-card">
        <div class="provider-mapping-list">
          ${formMappingMarkup()}
        </div>
        <div class="provider-mapping-footer">
          <button class="btn btn-outline-primary btn-sm" type="button" data-action="add-provider-model-row" ${canAddMoreRows ? "" : "disabled"}>
            <i class="bi bi-plus-lg"></i><span>${escapeHtml(t("providersAdd.addMapping"))}</span>
          </button>
        </div>
      </div>
    `;
  }

  function setProviderMappings(mappings = {}, options = {}) {
    providerFormMappings = normalizeMappings(mappings);
    providerFormRows = formMappingRowsFromMappings(providerFormMappings);
    if (Array.isArray(options.availableModels)) {
      providerAvailableModels = options.availableModels.slice();
    }
    openProviderSlotMenuIndex = null;
    openProviderModelMenuKey = null;
    renderProviderMappings();
  }

  function applyFetchedDefaultMapping(suggested = {}, availableModels = []) {
    const defaultModel = String(suggested?.default || "").trim();
    const nextMappings = { ...providerFormMappings };
    if (defaultModel) {
      nextMappings.default = defaultModel;
    }
    setProviderMappings(nextMappings, { availableModels });
  }

  function updateProviderModelInput(slotKey, value) {
    providerFormMappings[slotKey] = value.trim();
  }

  function moveProviderMappingRow(index, nextKey) {
    const prevKey = providerFormRows[index];
    if (!nextKey || prevKey === nextKey) return;
    const currentValue = providerFormMappings[prevKey] || "";
    providerFormRows[index] = nextKey;
    if (!providerFormMappings[nextKey]) {
      providerFormMappings[nextKey] = currentValue;
    }
    if (prevKey !== "default") {
      providerFormMappings[prevKey] = "";
    }
    openProviderSlotMenuIndex = null;
    renderProviderMappings();
  }

  function addProviderMappingRow() {
    const remaining = providerFormModelSlots
      .map((slot) => slot.key)
      .find((key) => !providerFormRows.includes(key));
    if (!remaining) return;
    providerFormRows = [...providerFormRows, remaining];
    openProviderSlotMenuIndex = null;
    openProviderModelMenuKey = null;
    renderProviderMappings();
  }

  function removeProviderMappingRow(index) {
    const key = providerFormRows[index];
    if (!key || key === "default") return;
    providerFormRows = providerFormRows.filter((_, rowIndex) => rowIndex !== index);
    providerFormMappings[key] = "";
    openProviderSlotMenuIndex = null;
    if (openProviderModelMenuKey === key) openProviderModelMenuKey = null;
    renderProviderMappings();
  }

  function toggleProviderSlotMenu(index) {
    openProviderSlotMenuIndex = openProviderSlotMenuIndex === index ? null : index;
    renderProviderMappings();
  }

  function closeProviderSlotMenu() {
    if (openProviderSlotMenuIndex === null) return;
    openProviderSlotMenuIndex = null;
    renderProviderMappings();
  }

  function toggleProviderModelMenu(rowKey) {
    openProviderModelMenuKey = openProviderModelMenuKey === rowKey ? null : rowKey;
    renderProviderMappings();
  }

  function closeProviderModelMenu() {
    if (openProviderModelMenuKey === null) return;
    openProviderModelMenuKey = null;
    renderProviderMappings();
  }

  function renderAuthSchemeControl() {
    const input = $("#providerAuth");
    const trigger = $("#providerAuthTrigger");
    const menu = $("#providerAuthMenu");
    const wrap = $("#providerAuthControl");
    if (!input || !trigger || !menu || !wrap) return;
    const value = providerAuthSchemes.includes(input.value) ? input.value : "bearer";
    input.value = value;
    $("span", trigger).textContent = value;
    trigger.setAttribute("aria-expanded", authSchemeMenuOpen ? "true" : "false");
    wrap.classList.toggle("open", authSchemeMenuOpen);
    menu.innerHTML = providerAuthSchemes.map((item) => `
      <button class="auth-scheme-option ${item === value ? "selected" : ""}" type="button" role="option" data-action="select-auth-scheme" data-value="${escapeHtml(item)}" aria-selected="${item === value ? "true" : "false"}">
        <span>${escapeHtml(item)}</span>
        ${item === value ? '<i class="bi bi-check2"></i>' : ""}
      </button>
    `).join("");
  }

  function setAuthSchemeValue(value) {
    const input = $("#providerAuth");
    if (!input) return;
    input.value = providerAuthSchemes.includes(value) ? value : "bearer";
    authSchemeMenuOpen = false;
    renderAuthSchemeControl();
  }

  function toggleAuthSchemeMenu() {
    authSchemeMenuOpen = !authSchemeMenuOpen;
    renderAuthSchemeControl();
  }

  function closeAuthSchemeMenu() {
    if (!authSchemeMenuOpen) return;
    authSchemeMenuOpen = false;
    renderAuthSchemeControl();
  }

  function renderPresetOptions(preset = null, mappings = null) {
    const container = $("#providerPresetOptions");
    if (!container) return;
    const modelOptions = preset?.modelOptions && typeof preset.modelOptions === "object"
      ? Object.entries(preset.modelOptions)
      : [];
    const requestOptionPresets = preset?.requestOptionPresets && typeof preset.requestOptionPresets === "object"
      ? Object.entries(preset.requestOptionPresets)
      : [];
    const options = [...modelOptions, ...requestOptionPresets];
    if (!options.length) {
      container.hidden = true;
      container.innerHTML = "";
      return;
    }
    const currentMappings = normalizeMappings(mappings || collectProviderMappings());
    container.hidden = false;
    container.innerHTML = options.map(([id, option]) => `
      <label class="preset-option-item">
        <input class="form-check-input" type="checkbox" data-preset-model-option="${escapeHtml(id)}" ${optionEnabled(option, currentMappings) ? "checked" : ""}>
        <span>
          <strong>${escapeHtml(option.label || id)}</strong>
          <small>${escapeHtml(option.description || "")}</small>
        </span>
      </label>
    `).join("");
  }

  function applyPresetModelOption(optionId, enabled) {
    const option = selectedPreset?.modelOptions?.[optionId] || selectedPreset?.requestOptionPresets?.[optionId];
    if (!option) return;
    const hasModels = option.models && typeof option.models === "object";
    const hasCapabilities = option.modelCapabilities && typeof option.modelCapabilities === "object";
    const mappings = option.models
      ? (enabled ? option.models : selectedPreset.models || emptyMappings())
      : collectProviderMappings();
    if (hasModels) {
      setProviderMappings(mappings);
    }
    if (hasCapabilities) {
      formModelCapabilities = enabled
        ? mergeCapabilities(formModelCapabilities || selectedPreset.modelCapabilities || {}, option.modelCapabilities)
        : clearCapabilities(formModelCapabilities, option.modelCapabilities);
    } else if (hasModels) {
      formModelCapabilities = normalizeCapabilities(enabled
        ? option.modelCapabilities || selectedPreset.modelCapabilities || {}
        : selectedPreset.modelCapabilities || {});
    }
    if (option.requestOptions) {
      formRequestOptions = enabled
        ? mergeRequestOptions(selectedPreset.requestOptions || {}, option.requestOptions)
        : clearRequestOptions(formRequestOptions, option.requestOptions);
    }
    renderPresetOptions(selectedPreset, mappings);
    showToast(`${option.label || optionId} ${t("providersAdd.optionApplied")}`);
  }

  function collectProviderMappings() {
    return normalizeMappings(providerFormMappings);
  }

  function providerPayloadFromForm(includeModels = true) {
    const apiKey = $("#providerApiKey").value.trim();
    const mappings = includeModels ? collectProviderMappings() : null;
    const payload = {
      name: $("#providerName").value.trim(),
      baseUrl: $("#providerBaseUrl").value.trim(),
      authScheme: $("#providerAuth").value,
      apiFormat: formApiFormat,
      extraHeaders: selectedPreset?.extraHeaders || {},
      modelCapabilities: mappings ? capabilitiesForCurrentMappings(mappings) : normalizeCapabilities(formModelCapabilities),
      requestOptions: normalizeRequestOptions(formRequestOptions),
    };
    if (apiKey) {
      payload.apiKey = apiKey;
    }
    if (includeModels) {
      payload.models = mappings;
    }
    return payload;
  }

  function providerCardMarkup(provider) {
    const mapping = [
      provider.mappings.default,
      provider.mappings.sonnet_4_6,
      provider.mappings.haiku_4_5,
      provider.mappings.opus_4_7,
    ]
      .filter(Boolean)
      .slice(0, 2)
      .join(" / ");
    const providerId = escapeHtml(provider.id);
    const providerName = escapeHtml(provider.name);
    const providerUrl = escapeHtml(provider.baseUrl);
    const providerHref = escapeHtml(safeHttpUrl(provider.baseUrl));
    const mappingText = escapeHtml(mapping || provider.apiFormat);
    return `
      <article class="provider-switch-card ${provider.default ? "active" : ""}" draggable="true" data-provider-id="${providerId}">
        <span class="drag-handle"><i class="bi bi-grip-vertical"></i></span>
        <span class="provider-logo">${iconMarkup(provider)}</span>
        <span class="provider-main">
          <strong>${providerName}</strong>
          <a class="truncate" href="${providerHref}" target="_blank" rel="noreferrer">${providerUrl}</a>
        </span>
        <span class="provider-meta truncate">${mappingText}</span>
        <span class="provider-actions">
          <button class="btn btn-primary compact-enable" type="button" data-action="set-default" data-id="${providerId}" ${provider.default ? "disabled" : ""}>
            <i class="bi bi-play-fill"></i><span>${provider.default ? t("status.default") : t("providers.enable")}</span>
          </button>
          <button class="icon-action" type="button" data-action="test-provider" data-id="${providerId}" title="${t("providers.testSpeed")}" aria-label="${t("providers.testSpeed")}"><i class="bi bi-lightning-charge"></i></button>
          <button class="icon-action" type="button" data-action="query-usage" data-id="${providerId}" title="${t("providers.usage")}" aria-label="${t("providers.usage")}"><i class="bi bi-wallet2"></i></button>
          <button class="icon-action" type="button" data-action="edit-provider" data-id="${providerId}" title="${t("common.edit")}" aria-label="${t("common.edit")}"><i class="bi bi-pencil-square"></i></button>
          <button class="icon-action" type="button" data-action="copy-url" data-url="${providerUrl}" title="${t("common.copy")}" aria-label="${t("common.copy")}"><i class="bi bi-copy"></i></button>
          <a class="icon-action" href="#proxy" title="${t("nav.proxy")}" aria-label="${t("nav.proxy")}"><i class="bi bi-terminal"></i></a>
          <button class="icon-action danger" type="button" data-action="delete-provider" data-id="${providerId}" title="${t("common.delete")}" aria-label="${t("common.delete")}"><i class="bi bi-trash"></i></button>
        </span>
        <span class="provider-feedback">
          <span class="speed-result inline" data-speed-for="${providerId}"></span>
          <span class="usage-result inline" data-usage-for="${providerId}"></span>
        </span>
      </article>
    `;
  }

  function providerPresetCardMarkup(preset, added = false) {
    const presetId = escapeHtml(preset.id);
    return `
      <button class="provider-switch-card preset-card ${added ? "added" : ""}" type="button" data-action="new-from-preset" data-preset="${presetId}" ${added ? "disabled" : ""}>
        <span class="drag-handle preset-plus"><i class="bi ${added ? "bi-check2" : "bi-plus-lg"}"></i></span>
        <span class="provider-logo">${iconMarkup(preset)}</span>
        <span class="provider-main"><strong>${escapeHtml(preset.name)}</strong><span class="truncate">${escapeHtml(preset.baseUrl)}</span></span>
        <span class="provider-meta">${escapeHtml(preset.apiFormat)}</span>
        <span class="provider-actions"><span class="compact-enable ghost"><i class="bi ${added ? "bi-check2" : "bi-plus-lg"}"></i><span>${added ? t("providers.added") : t("providers.add")}</span></span></span>
      </button>
    `;
  }

  function dashboardPresetSectionMarkup(providers, presets) {
    const available = presets.filter((preset) => !presetExists(preset, providers));
    if (!available.length) return "";
    return `
      <section class="dashboard-preset-section" aria-label="${escapeHtml(t("dashboard.availablePresets"))}">
        <div class="section-title-row compact">
          <div>
            <h2>${escapeHtml(t("dashboard.availablePresets"))}</h2>
            <p>${escapeHtml(t("dashboard.availablePresetsHint"))}</p>
          </div>
        </div>
        <div class="provider-preset-grid">
          ${available.map((preset) => providerPresetCardMarkup(preset)).join("")}
        </div>
      </section>
    `;
  }

  function getDragAfterElement(container, y) {
    const items = [...container.querySelectorAll("[data-provider-id]:not(.dragging)")];
    return items.reduce((closest, child) => {
      const box = child.getBoundingClientRect();
      const offset = y - box.top - box.height / 2;
      if (offset < 0 && offset > closest.offset) return { offset, element: child };
      return closest;
    }, { offset: Number.NEGATIVE_INFINITY, element: null }).element;
  }

  function enableProviderReorder(listEl) {
    if (!listEl || listEl.dataset.reorderBound === "1") return;
    listEl.dataset.reorderBound = "1";

    listEl.addEventListener("dragstart", (event) => {
      const card = event.target.closest("[data-provider-id]");
      if (!card) return;
      card.classList.add("dragging");
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", card.dataset.providerId);
    });

    listEl.addEventListener("dragover", (event) => {
      const dragging = listEl.querySelector(".dragging");
      if (!dragging) return;
      event.preventDefault();
      const afterElement = getDragAfterElement(listEl, event.clientY);
      if (afterElement) {
        listEl.insertBefore(dragging, afterElement);
      } else {
        listEl.appendChild(dragging);
      }
    });

    listEl.addEventListener("drop", async (event) => {
      const dragging = listEl.querySelector(".dragging");
      if (!dragging) return;
      event.preventDefault();
      dragging.classList.remove("dragging");
      const providerIds = $all("[data-provider-id]", listEl).map((item) => item.dataset.providerId);
      try {
        await CCApi.reorderProviders(providerIds);
        showToast(t("toast.providersReordered"));
        await renderProviders();
        if (routeFromHash() === "dashboard") await renderDashboard();
      } catch (error) {
        console.error(error);
        if (routeFromHash() === "dashboard") {
          await renderDashboard();
        } else {
          await renderProviders();
        }
        showToast(error.message || t("toast.requestFailed"));
      }
    });

    listEl.addEventListener("dragend", (event) => {
      event.target.closest("[data-provider-id]")?.classList.remove("dragging");
    });
  }

  async function renderProviderCards(targetSelector, options = {}) {
    const target = $(targetSelector);
    if (!target) return;
    const providers = await CCApi.getProviders();
    const providerList = providers.length
      ? `<div class="provider-configured-list" data-provider-list>${providers.map(providerCardMarkup).join("")}</div>`
      : "";
    if (!providers.length) {
      const presets = await CCApi.getPresets();
      target.innerHTML = `<div class="provider-preset-grid">${presets.map((preset) => providerPresetCardMarkup(preset)).join("")}</div>`;
      return;
    }
    target.innerHTML = providerList;
    enableProviderReorder($("[data-provider-list]", target));
    if (options.includePresets) {
      const presets = await CCApi.getPresets();
      target.innerHTML = `${providerList}${dashboardPresetSectionMarkup(providers, presets)}`;
      enableProviderReorder($("[data-provider-list]", target));
    } else {
      target.innerHTML = providerList;
      enableProviderReorder($("[data-provider-list]", target));
    }
  }
