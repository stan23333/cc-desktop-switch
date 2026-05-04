  let currentTheme = "default";

  function normalizeTheme(theme) {
    if (!theme || theme === "light" || theme === "auto") return "default";
    return availableThemes.includes(theme) ? theme : "default";
  }

  function applyTheme(theme) {
    if (theme === "toggle") {
      theme = currentTheme === "dark" ? "default" : "dark";
    }
    const normalized = normalizeTheme(theme);
    currentTheme = normalized;
    document.documentElement.setAttribute("data-bs-theme", normalized === "dark" ? "dark" : "light");
    document.documentElement.setAttribute("data-theme-palette", normalized);
    $all(".theme-segment .btn").forEach((button) => {
      const active = button.dataset.themeAction === normalized;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", active ? "true" : "false");
    });
    const icon = $("[data-theme-action='toggle'] i");
    if (icon) icon.className = normalized === "dark" ? "bi bi-sun-fill" : "bi bi-moon-stars-fill";
    return normalized;
  }

  async function saveSettingsFromForm() {
    const settings = {
      theme: currentTheme,
      proxyPort: Number($("#settingsProxyPort").value),
      adminPort: Number($("#settingsAdminPort").value),
      autoStart: $("#autoStart").checked,
      exposeAllProviderModels: $("#exposeAllProviderModels")?.checked || false,
      updateUrl: $("#settingsUpdateUrl").value.trim(),
      upstreamProxy: $("#settingsUpstreamProxy").value.trim(),
      upstreamProxyEnabled: $("#settingsUpstreamProxyEnabled").checked,
    };
    await CCApi.saveSettings(settings);
    $("#proxyPort").value = settings.proxyPort;
    renderModelMenuModeState(settings);
  }

  function formatUsageItems(result) {
    if (result.supported === false) return result.message;
    if (!result.items || !result.items.length) return result.message || t("providers.usageUnavailable");
    return result.items.map((item) => {
      const unit = item.unit ? ` ${item.unit}` : "";
      if (item.remaining !== null && item.remaining !== undefined) {
        return `${item.label}: ${item.remaining}${unit}`;
      }
      if (item.used !== null && item.used !== undefined) {
        return `${item.label}: ${item.used}${unit}`;
      }
      return item.label;
    }).join(" · ");
  }

  function downloadJson(filename, data) {
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  async function refreshBackupList() {
    const target = $("#backupList");
    if (!target) return;
    try {
      const backups = await CCApi.listBackups();
      target.innerHTML = backups.length
        ? backups.slice(0, 5).map((item) => `<span>${escapeHtml(item.name)}</span><time>${escapeHtml(item.createdAt)}</time>`).join("")
        : `<span>${t("settings.noBackups")}</span>`;
    } catch (error) {
      target.innerHTML = `<span>${t("settings.backupLoadFailed")}</span>`;
    }
  }

  async function importConfigFile(file) {
    if (!file) return;
    if (!(await confirmAction(t("confirm.ccswitchImport")))) return;
    try {
      const text = await file.text();
      const configData = JSON.parse(text);
      await CCApi.importConfig(configData);
      await renderRoute(routeFromHash());
      showToast(t("toast.configImported"));
    } catch (error) {
      console.error(error);
      showToast(error.message || t("toast.configImportFailed"));
    } finally {
      const input = $("#configImportFile");
      if (input) input.value = "";
    }
  }

  function ccSwitchStatusMessage(providers = ccSwitchCandidates) {
    const supported = providers.filter((item) => item.supported).length;
    const unsupported = providers.length - supported;
    if (!providers.length) return t("settings.ccswitchNotFound");
    return formatI18n("settings.ccswitchFound", { supported, unsupported });
  }

  function translateCcSwitchReason(reason = "") {
    const normalized = String(reason || "").trim();
    if (!normalized) return "";
    if (normalized === "没有发现 API 地址，可能是官方登录或空配置。") {
      return t("settings.ccswitchReason.noBaseUrl");
    }
    if (normalized === "这是 CC-Switch 本机代理地址，不能作为上游 API 导入。") {
      return t("settings.ccswitchReason.localProxy");
    }
    if (normalized === "没有发现 API Key。") {
      return t("settings.ccswitchReason.noApiKey");
    }
    if (normalized === "OpenAI Chat 格式本轮不自动导入，避免转换兼容风险。") {
      return t("settings.ccswitchReason.openaiChat");
    }
    if (normalized === "OpenAI Responses 格式暂未适配，暂不自动导入。") {
      return t("settings.ccswitchReason.openaiResponses");
    }
    const unsupportedMatch = normalized.match(/^(.+?) 格式暂不支持自动导入。$/);
    if (unsupportedMatch) {
      return formatI18n("settings.ccswitchReason.unsupportedFormat", { format: unsupportedMatch[1] });
    }
    return normalized;
  }

  function renderCcSwitchImportList(providers = ccSwitchCandidates, message = "") {
    const target = $("#ccSwitchImportList");
    if (!target) return;
    if (!providers.length) {
      target.innerHTML = `<p class="ccswitch-import-empty">${escapeHtml(message || t("settings.ccswitchNotFound"))}</p>`;
      return;
    }
    target.innerHTML = `
      <p class="ccswitch-import-summary">${escapeHtml(message || ccSwitchStatusMessage(providers))}</p>
      ${providers.map((provider) => {
        const statusLabel = provider.supported ? t("settings.ccswitchSupported") : t("settings.ccswitchUnsupported");
        const statusIcon = provider.supported ? "bi-check2-circle" : "bi-slash-circle";
        const model = provider.models?.default || provider.models?.sonnet_4_6 || provider.models?.sonnet || "";
        const translatedReason = translateCcSwitchReason(provider.reason);
        return `
          <article class="ccswitch-import-item ${provider.supported ? "supported" : "unsupported"}">
            <div>
              <strong>${escapeHtml(provider.name)}</strong>
              <span class="truncate">${escapeHtml(provider.baseUrl || translatedReason || provider.apiFormat)}</span>
              ${model ? `<small>${escapeHtml(model)}</small>` : ""}
            </div>
            <span class="ccswitch-import-secret">${escapeHtml(provider.hasApiKey ? provider.apiKeyPreview : "")}</span>
            <span class="ccswitch-import-status"><i class="bi ${statusIcon}"></i>${escapeHtml(statusLabel)}</span>
            ${translatedReason ? `<p>${escapeHtml(translatedReason)}</p>` : ""}
          </article>
        `;
      }).join("")}
    `;
  }

  function renderProviderCompatibilityList(result) {
    const target = $("#providerCompatibilityList");
    if (!target) return;
    const providers = result?.providers || [];
    if (!providers.length) {
      target.innerHTML = `<p class="compatibility-empty">${escapeHtml(t("settings.compatibilityEmpty"))}</p>`;
      return;
    }
    target.innerHTML = providers.map((provider) => `
      <article class="compatibility-item ${escapeHtml(provider.level)}">
        <div>
          <strong>${escapeHtml(provider.name)}</strong>
          <span>${escapeHtml(provider.message)}</span>
        </div>
        <em>${escapeHtml(provider.apiFormat)}</em>
      </article>
    `).join("");
  }

  async function refreshCcSwitchImportStatus() {
    const target = $("#ccSwitchImportList");
    if (!target) return;
    try {
      const status = await CCApi.getCcSwitchStatus();
      if (!status.found) {
        ccSwitchCandidates = [];
        renderCcSwitchImportList([], t("settings.ccswitchNotFound"));
        return;
      }
      const result = await CCApi.getCcSwitchProviders();
      ccSwitchCandidates = result.providers || [];
      renderCcSwitchImportList(ccSwitchCandidates, formatI18n("settings.ccswitchFound", {
        supported: result.supportedCount || 0,
        unsupported: result.unsupportedCount || 0,
      }));
    } catch (error) {
      console.error(error);
      ccSwitchCandidates = [];
      renderCcSwitchImportList([], error.message || t("settings.ccswitchNotFound"));
    }
  }

  async function importCcSwitchProviders(actionEl) {
    if (!ccSwitchCandidates.length) {
      await refreshCcSwitchImportStatus();
    }
    const ids = ccSwitchCandidates.filter((item) => item.supported).map((item) => item.id);
    if (!ids.length) {
      showToast(t("settings.ccswitchNoSupported"));
      return;
    }
    if (!(await confirmAction(t("confirm.configImport")))) return;
    actionEl.disabled = true;
    try {
      const result = await CCApi.importCcSwitchProviders(ids, false);
      await refreshBackupList();
      await refreshCcSwitchImportStatus();
      if (routeFromHash() === "dashboard") await renderDashboard();
      if (routeFromHash() === "providers") await renderProviders();
      showToast(formatI18n("settings.ccswitchImported", { count: result.imported?.length || 0 }));
    } finally {
      actionEl.disabled = false;
    }
  }
