  async function saveProviderFromForm() {
    const payload = providerPayloadFromForm(true);
    if (editingProviderId) {
      const provider = await CCApi.updateProvider(editingProviderId, payload);
      editingProviderId = provider.id || editingProviderId;
      return provider;
    }
    const provider = await CCApi.addProvider(payload);
    editingProviderId = provider.id;
    return provider;
  }

  async function applyProviderToDesktop(actionEl) {
    const form = $("#providerForm");
    if (form && !form.reportValidity()) {
      form.querySelector(":invalid")?.focus();
      showToast(t("toast.formInvalid"));
      return;
    }
    if (!(await confirmAction(t("confirm.providerApplyDesktop")))) return;

    actionEl.disabled = true;
    try {
      showToast(t("toast.providerApplyingDesktop"));
      const provider = await saveProviderFromForm();
      await CCApi.setDefaultProvider(provider.id);
      const desktopResult = await CCApi.configureDesktop();
      if (desktopResult.requiresProxy) {
        await CCApi.startProxy();
      }
      editingProviderId = null;
      selectedPreset = null;
      window.location.hash = "dashboard";
      showToast(t("toast.providerAppliedDesktop"));
      showRestartReminder();
    } finally {
      actionEl.disabled = false;
    }
  }

  async function handleAction(target) {
    const action = target.closest("[data-action]")?.dataset.action;
    if (!action) return;
    const actionEl = target.closest("[data-action]");

    if (action === "toggle-key") {
      const input = $("#providerApiKey");
      input.type = input.type === "password" ? "text" : "password";
      actionEl.innerHTML = `<i class="bi ${input.type === "password" ? "bi-eye" : "bi-eye-slash"}"></i>`;
    }

    try {
      if (action === "set-default") {
        const result = await CCApi.setDefaultProvider(actionEl.dataset.id);
        if (result.desktopSync?.requiresProxy) {
          await CCApi.startProxy();
        }
        await renderProviderCards("#dashboardProviderCards", { includePresets: true });
        await renderProviders();
        await renderDashboard();
        const desktopSync = result.desktopSync || {};
        if (desktopSync.attempted && desktopSync.success) {
          showToast(t("toast.defaultUpdatedDesktop"));
          await confirmRestartClaudeAfterEnable();
        } else if (desktopSync.attempted && desktopSync.success === false) {
          showToast(t("toast.defaultUpdatedDesktopFailed"));
        } else {
          showToast(t("toast.defaultUpdated"));
          await confirmRestartClaudeAfterEnable();
        }
      }

      if (action === "new-from-preset") {
        const presets = await CCApi.getPresets();
        selectedPreset = presets.find((item) => item.id === actionEl.dataset.preset) || null;
        editingProviderId = null;
        window.location.hash = "providers/add";
      }

      if (action === "edit-provider") {
        editingProviderId = actionEl.dataset.id;
        selectedPreset = null;
        window.location.hash = "providers/add";
      }

      if (action === "copy-url") {
        await navigator.clipboard.writeText(actionEl.dataset.url || "");
        showToast(t("toast.copied"));
      }

      if (action === "test-provider") {
        const resultEl = $(`[data-speed-for="${actionEl.dataset.id}"]`);
        actionEl.disabled = true;
        if (resultEl) {
          resultEl.textContent = t("providers.testing");
          resultEl.classList.remove("bad");
        }
        try {
          const result = await CCApi.testProvider(actionEl.dataset.id);
          if (resultEl) {
            resultEl.textContent = result.message || `${result.latencyMs} ms`;
            resultEl.classList.toggle("bad", result.ok === false);
          }
          showToast(result.message || t("providers.testDone"));
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "query-usage") {
        const resultEl = $(`[data-usage-for="${actionEl.dataset.id}"]`) || $(`[data-speed-for="${actionEl.dataset.id}"]`);
        actionEl.disabled = true;
        if (resultEl) {
          resultEl.textContent = t("providers.usageQuerying");
          resultEl.classList.remove("bad");
        }
        try {
          const result = await CCApi.queryProviderUsage(actionEl.dataset.id);
          const message = formatUsageItems(result);
          if (resultEl) {
            resultEl.textContent = message;
            resultEl.classList.toggle("bad", result.ok === false || result.supported === false);
          }
          showToast(message);
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "test-provider-form") {
        const resultEl = $("#formSpeedResult");
        actionEl.disabled = true;
        resultEl.textContent = t("providers.testing");
        resultEl.classList.remove("bad");
        try {
          const hasTypedKey = !!$("#providerApiKey").value.trim();
          const result = editingProviderId && !hasTypedKey
            ? await CCApi.testProvider(editingProviderId)
            : await CCApi.testProviderPayload(providerPayloadFromForm(false));
          resultEl.textContent = result.message || `${result.latencyMs} ms`;
          resultEl.classList.toggle("bad", result.ok === false);
          showToast(result.message || t("providers.testDone"));
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "fetch-form-models") {
        const resultEl = $("#providerModelFetchResult");
        actionEl.disabled = true;
        if (resultEl) {
          resultEl.textContent = t("models.fetching");
          resultEl.classList.remove("bad");
        }
        try {
          const hasTypedKey = !!$("#providerApiKey").value.trim();
          const result = editingProviderId && !hasTypedKey
            ? await CCApi.autofillProviderModels(editingProviderId)
            : await CCApi.fetchProviderModelsPayload(providerPayloadFromForm(false));
          if (result.success === false) {
            throw new Error(result.message || t("models.fetchFailedManual"));
          }
          providerAvailableModels = Array.isArray(result.models) ? result.models.slice() : [];
          applyFetchedDefaultMapping(result.suggested || {}, providerAvailableModels);
          if (resultEl) resultEl.textContent = t("models.fetchSuccess");
          showToast(t("toast.modelsAutofilled"));
        } catch (error) {
          providerAvailableModels = [];
          renderProviderMappings();
          if (resultEl) {
            resultEl.textContent = error.message || t("models.fetchFailedManual");
            resultEl.classList.add("bad");
          }
          showToast(error.message || t("toast.requestFailed"));
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "add-provider-model-row") {
        addProviderMappingRow();
      }

      if (action === "remove-provider-model-row") {
        removeProviderMappingRow(Number(actionEl.dataset.rowIndex));
      }

      if (action === "check-model") {
        const rowKey = actionEl.dataset.rowKey;
        const model = providerFormMappings[rowKey] || "";
        if (!model || !editingProviderId) return;
        actionEl.disabled = true;
        actionEl.innerHTML = `<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>`;
        try {
          const result = await CCApi.checkModelAvailability(editingProviderId, model);
          showMappingCheckAlert(model, result.available, result.message);
        } catch (error) {
          showMappingCheckAlert(model, false, error.message || t("toast.requestFailed"));
        } finally {
          actionEl.disabled = false;
          actionEl.innerHTML = escapeHtml(t("providersAdd.checkModel"));
        }
      }

      if (action === "toggle-provider-model-slot-menu") {
        toggleProviderSlotMenu(Number(actionEl.dataset.rowIndex));
      }

      if (action === "toggle-baseurl-menu") {
        toggleBaseUrlMenu();
      }

      if (action === "select-baseurl-option") {
        setBaseUrlValue(actionEl.dataset.baseurlValue || "");
      }

      if (action === "select-provider-model-slot") {
        moveProviderMappingRow(Number(actionEl.dataset.rowIndex), actionEl.dataset.slotKey);
        renderPresetOptions(selectedPreset, collectProviderMappings());
      }

      if (action === "toggle-provider-model-menu") {
        toggleProviderModelMenu(actionEl.dataset.rowKey);
      }

      if (action === "select-provider-model-option") {
        const rowKey = openProviderModelMenuKey;
        if (rowKey) {
          updateProviderModelInput(rowKey, actionEl.dataset.modelValue || "");
          closeProviderModelMenu();
          renderPresetOptions(selectedPreset, collectProviderMappings());
        }
      }

      if (action === "toggle-auth-scheme-menu") {
        toggleAuthSchemeMenu();
      }

      if (action === "select-auth-scheme") {
        setAuthSchemeValue(actionEl.dataset.value);
      }

      if (action === "delete-provider") {
        pendingDeleteId = actionEl.dataset.id;
        deleteModal.show();
      }

      if (action === "save-models") {
        const mappings = {};
        $all("[data-model-input]").forEach((input) => {
          mappings[input.dataset.modelInput] = input.value.trim();
        });
        const defaultKey = $("#defaultModel")?.value || "sonnet";
        mappings.default = mappings[defaultKey]
          || mappings.sonnet_4_6
          || mappings.sonnet_4_5
          || mappings.haiku_4_5
          || mappings.opus_4_7
          || mappings.opus_4_6
          || mappings.opus_3
          || mappings.sonnet
          || mappings.haiku
          || mappings.opus
          || "";
        await CCApi.saveModelMappings($("#modelProvider").value, mappings);
        showToast(t("toast.modelsSaved"));
      }

      if (action === "fetch-models") {
        const providerId = $("#modelProvider").value;
        const resultEl = $("#modelFetchResult");
        actionEl.disabled = true;
        if (resultEl) {
          resultEl.textContent = t("models.fetching");
          resultEl.classList.remove("bad");
        }
        try {
          const result = await CCApi.autofillProviderModels(providerId);
          if (result.success === false) {
            throw new Error(result.message || t("models.fetchFailedManual"));
          }
          await renderMappingCards();
          if (resultEl) {
            resultEl.textContent = `${t("models.fetched")} ${result.models.length}`;
          }
          showToast(t("toast.modelsAutofilled"));
        } catch (error) {
          if (resultEl) {
            resultEl.textContent = error.message || t("models.fetchFailedManual");
            resultEl.classList.add("bad");
          }
          showToast(error.message || t("toast.requestFailed"));
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "reset-models") {
        await renderMappingCards();
        showToast(t("toast.modelsReset"));
      }

      if (action === "apply-desktop") {
        if (!(await confirmAction(t("confirm.desktopApply")))) return;
        await CCApi.configureDesktop();
        await renderDesktop();
        showToast(t("toast.desktopApplied"));
        showRestartReminder();
      }

      if (action === "restart-claude") {
        if (!(await confirmAction(t("confirm.restartClaude")))) return;
        await restartClaudeDesktopFromUi(actionEl);
      }

      if (action === "clear-desktop") {
        if (!(await confirmAction(t("confirm.desktopClear")))) return;
        await CCApi.clearDesktop();
        const route = routeFromHash();
        if (route === "dashboard") {
          await renderDashboard();
        } else if (route === "desktop") {
          await renderDesktop();
        }
        showToast(t("toast.desktopCleared"));
      }

      if (action === "proxy-start") {
        await CCApi.startProxy($("#proxyPort") ? $("#proxyPort").value : 18080);
        await renderProxy();
        await renderDashboard();
        showToast(t("toast.proxyStarted"));
      }

      if (action === "proxy-stop") {
        await CCApi.stopProxy();
        await renderProxy();
        await renderDashboard();
        showToast(t("toast.proxyStopped"));
      }

      if (action === "clear-logs") {
        await CCApi.clearLogs();
        await renderProxy();
        showToast(t("toast.logsCleared"));
      }

      if (action === "view-logs") {
        window.location.hash = "proxy";
      }

      if (action === "open-feedback") {
        openFeedbackModal();
      }

      if (action === "open-ccswitch-import") {
        openCcSwitchImportSettings();
      }

      if (action === "toggle-model-menu-mode") {
        const settings = await CCApi.getSettings();
        const next = !settings.exposeAllProviderModels;
        const saved = await CCApi.saveSettings({ exposeAllProviderModels: next });
        renderModelMenuModeState(saved);
        showToast(next ? t("toast.allModelsEnabled") : t("toast.singleModelEnabled"));
      }

      if (action === "check-provider-compatibility") {
        actionEl.disabled = true;
        try {
          const result = await CCApi.getProviderCompatibility();
          renderProviderCompatibilityList(result);
          showToast(t("toast.compatibilityChecked"));
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "detect-proxy") {
        actionEl.disabled = true;
        try {
          const detected = await CCApi.detectLocalProxy();
          if (detected) {
            $("#settingsUpstreamProxy").value = detected;
            await saveSettingsFromForm();
            showToast(t("toast.proxyDetected"));
          } else {
            showToast(t("toast.proxyNotDetected"));
          }
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "check-update") {
        const result = await CCApi.checkUpdate($("#settingsUpdateUrl").value.trim());
        updateCheckCache = result;
        renderUpdateBadge(result);
        const message = result.updateAvailable
          ? `${t("toast.updateAvailable")} ${result.latestVersion}`
          : `${t("toast.noUpdate")} ${result.currentVersion}`;
        const status = $("#updateStatus");
        if (status) {
          status.textContent = message;
          status.classList.toggle("available", !!result.updateAvailable);
        }
        showToast(message);
      }

      if (action === "install-update") {
        if (!updateCheckCache?.updateAvailable) {
          updateCheckCache = await CCApi.checkUpdate($("#settingsUpdateUrl")?.value.trim() || "");
          renderUpdateBadge(updateCheckCache);
        }
        if (!updateCheckCache?.updateAvailable) {
          const message = `${t("toast.noUpdate")} ${updateCheckCache?.currentVersion || ""}`.trim();
          const status = $("#updateStatus");
          if (status) {
            status.textContent = message;
            status.classList.remove("available");
          }
          showToast(message);
          return;
        }
        if (!(await confirmAction(t("confirm.installUpdate")))) return;
        let keepBusyState = false;
        const status = $("#updateStatus");
        let progressPoll = null;
        setUpdateInstallPhase("downloading");
        if (status) {
          status.textContent = t("toast.updateDownloading");
          status.classList.add("available");
          status.style.setProperty("--progress", "0%");
        }
        const startProgressPoll = () => {
          progressPoll = setInterval(async () => {
            try {
              const progress = await CCApi.getUpdateProgress();
              if (progress.active && status) {
                status.style.setProperty("--progress", `${progress.percent}%`);
                if (progress.message) {
                  status.textContent = progress.message;
                }
              }
            } catch (e) {}
          }, 300);
        };
        const stopProgressPoll = () => {
          if (progressPoll) {
            clearInterval(progressPoll);
            progressPoll = null;
          }
          if (status) {
            status.style.setProperty("--progress", "0%");
          }
        };
        startProgressPoll();
        try {
          const result = await CCApi.installUpdate($("#settingsUpdateUrl")?.value.trim() || "");
          updateCheckCache = result;
          keepBusyState = !!result.quitRequested;
          setUpdateInstallPhase(keepBusyState ? "installing" : "idle");
          renderUpdateBadge(result);
          const message = result.message || t("toast.updateInstallerStarted");
          if (status) {
            status.textContent = message;
            status.classList.toggle("available", !!result.updateAvailable);
          }
          showToast(message);
        } catch (error) {
          setUpdateInstallPhase("idle");
          if (status) {
            status.textContent = "下载失败，请重试";
            status.classList.remove("available");
          }
          throw error;
        } finally {
          stopProgressPoll();
          if (!keepBusyState) setUpdateInstallPhase("idle");
        }
      }

      if (action === "backup-config") {
        await CCApi.createBackup();
        await refreshBackupList();
        showToast(t("toast.configBackedUp"));
      }

      if (action === "export-config") {
        const data = await CCApi.exportConfig();
        const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
        downloadJson(`cc-desktop-switch-config-${stamp}.json`, data);
        showToast(t("toast.configExported"));
      }

      if (action === "choose-import-config") {
        $("#configImportFile").click();
      }

      if (action === "detect-ccswitch") {
        actionEl.disabled = true;
        try {
          await refreshCcSwitchImportStatus();
          showToast(ccSwitchStatusMessage());
        } finally {
          actionEl.disabled = false;
        }
      }

      if (action === "import-ccswitch") {
        await importCcSwitchProviders(actionEl);
      }

      if (action === "apply-provider-desktop") {
        await applyProviderToDesktop(actionEl);
      }

      if (action === "detect-format") {
        await handleDetectFormat();
      }

    } catch (error) {
      console.error(error);
      showToast(error.message || t("toast.requestFailed"));
    }
  }
