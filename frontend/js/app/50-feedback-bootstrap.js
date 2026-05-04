  async function fillPreset(presetId) {
    if (!presetCache.length) presetCache = await CCApi.getPresets();
    const preset = presetCache.find((item) => item.id === presetId);
    if (!preset) return;
    editingProviderId = null;
    applyPresetToForm(preset);
  }

  function openFeedbackModal() {
    const modalEl = $("#feedbackModal");
    if (!modalEl) return;
    $("#feedbackTitle").value = "";
    $("#feedbackBody").value = "";
    $("#feedbackIncludeDiagnostics").checked = true;
    feedbackAttachments = [];
    renderFeedbackAttachments();
    if (!feedbackBsModal) feedbackBsModal = new bootstrap.Modal(modalEl);
    feedbackBsModal.show();
  }

  function renderFeedbackAttachments() {
    const list = $("#feedbackAttachmentList");
    if (!list) return;
    list.innerHTML = feedbackAttachments
      .map((attachment, index) => `
        <li class="feedback-attachment-item">
          <span>${escapeHtml(attachment.name)}</span>
          <small>${escapeHtml(formatBytes(attachment.size))}</small>
          <button type="button" class="btn-link" data-feedback-attachment="${index}" aria-label="${escapeHtml(t("common.delete"))}">×</button>
        </li>
      `)
      .join("");
    $all("[data-feedback-attachment]", list).forEach((button) => {
      button.addEventListener("click", () => {
        feedbackAttachments.splice(Number(button.dataset.feedbackAttachment), 1);
        renderFeedbackAttachments();
      });
    });
  }

  function addFeedbackFiles(files) {
    if (!files || !files.length) return;
    const maxFileBytes = 5 * 1024 * 1024;
    for (const file of files) {
      if (file.size > maxFileBytes) {
        showToast(formatI18n("feedback.tooLargeFile", { name: file.name }));
        continue;
      }
      feedbackAttachments.push({ name: file.name, size: file.size, file });
    }
    renderFeedbackAttachments();
  }

  function formatBytes(size) {
    if (size < 1024) return `${size}B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)}KB`;
    return `${(size / 1024 / 1024).toFixed(2)}MB`;
  }

  function fileToBase64(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result || "");
        const comma = result.indexOf(",");
        resolve(comma >= 0 ? result.slice(comma + 1) : result);
      };
      reader.onerror = () => reject(reader.error || new Error("FileReader failed"));
      reader.readAsDataURL(file);
    });
  }

  async function submitFeedback() {
    const titleEl = $("#feedbackTitle");
    const bodyEl = $("#feedbackBody");
    const submitBtn = $("#feedbackSubmitBtn");
    if (!bodyEl || !submitBtn) return;

    const title = (titleEl?.value || "").trim();
    const body = bodyEl.value.trim();
    if (!body) {
      showToast(t("feedback.bodyRequired"));
      bodyEl.focus();
      return;
    }

    submitBtn.disabled = true;
    const originalText = submitBtn.textContent;
    submitBtn.textContent = t("feedback.submitting");

    try {
      const attachments = [];
      for (const attachment of feedbackAttachments) {
        try {
          const contentB64 = await fileToBase64(attachment.file);
          const isImage = /^image\//.test(attachment.file.type || "");
          const safeName = String(attachment.name || `attachment-${Date.now()}.bin`)
            .replace(/[\x00-\x1f\\/]/g, "_")
            .slice(0, 200);
          attachments.push({
            kind: isImage ? "screenshot" : "log",
            name: safeName,
            content_type: attachment.file.type || "application/octet-stream",
            content_b64: contentB64,
          });
        } catch (error) {
          console.warn("[feedback] skipped attachment:", error, attachment);
        }
      }

      const result = await CCApi.submitFeedback({
        title,
        body,
        include_diagnostics: $("#feedbackIncludeDiagnostics").checked,
        attachments,
      });
      if (feedbackBsModal) feedbackBsModal.hide();
      showToast(formatI18n("feedback.successToast", { id: result.id || "" }));
    } catch (error) {
      console.error("[feedback] submit failed:", error);
      let message = error && error.message ? error.message : String(error);
      if (message.includes("did not match the expected pattern")) {
        message = "请求体构造异常，请重试或去掉附件";
      }
      showToast(formatI18n("feedback.failToast", { message }));
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = originalText;
    }
  }

  function bindFeedbackEvents() {
    const dropzone = $("#feedbackDropzone");
    const fileInput = $("#feedbackFiles");
    if (dropzone && fileInput) {
      dropzone.addEventListener("click", (event) => {
        if (event.target.closest(".feedback-attachment-item")) return;
        fileInput.click();
      });
      fileInput.addEventListener("change", () => {
        addFeedbackFiles(Array.from(fileInput.files));
        fileInput.value = "";
      });
      dropzone.addEventListener("dragover", (event) => {
        event.preventDefault();
        dropzone.classList.add("dragover");
      });
      dropzone.addEventListener("dragleave", () => dropzone.classList.remove("dragover"));
      dropzone.addEventListener("drop", (event) => {
        event.preventDefault();
        dropzone.classList.remove("dragover");
        addFeedbackFiles(Array.from(event.dataTransfer.files));
      });
    }

    document.addEventListener("paste", (event) => {
      const modalEl = $("#feedbackModal");
      if (!modalEl?.classList.contains("show")) return;
      const items = event.clipboardData?.items || [];
      for (const item of items) {
        if (item.kind === "file" && /^image\//.test(item.type)) {
          const file = item.getAsFile();
          if (file) {
            addFeedbackFiles([new File([file], file.name || `pasted-${Date.now()}.png`, { type: file.type })]);
          }
        }
      }
    });

    const submitBtn = $("#feedbackSubmitBtn");
    if (submitBtn) submitBtn.addEventListener("click", submitFeedback);
  }

  function bindEvents() {
    window.addEventListener("hashchange", () => renderRoute(routeFromHash()));
    window.addEventListener("cc:i18n", () => renderRoute(routeFromHash()));
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (currentTheme === "dark") applyTheme("dark");
    });

    document.addEventListener("click", async (event) => {
      if (!event.target.closest(".mapping-slot-menu-wrap")) {
        closeProviderSlotMenu();
      }
      if (!event.target.closest(".baseurl-input-wrap")) {
        closeBaseUrlMenu();
      }
      if (!event.target.closest(".provider-model-input-wrap")) {
        closeProviderModelMenu();
      }
      if (!event.target.closest(".auth-scheme-menu-wrap")) {
        closeAuthSchemeMenu();
      }
      const langButton = event.target.closest("[data-lang]");
      if (langButton) CCI18n.apply(langButton.dataset.lang);
      const addLink = event.target.closest("a[href='#providers/add']");
      if (addLink) {
        editingProviderId = null;
        selectedPreset = null;
        updatePresetSelection();
      }
      const themeButton = event.target.closest("[data-theme-action]");
      if (themeButton) {
        const nextTheme = applyTheme(themeButton.dataset.themeAction);
        await CCApi.saveSettings({ theme: nextTheme });
      }
      const formatButton = event.target.closest("[data-api-format]");
      if (formatButton) {
        event.preventDefault();
        setFormApiFormat(formatButton.dataset.apiFormat);
        protocolDetected = true;
        updateApplyButtonState();
        showToast(formatButton.dataset.apiFormat === "openai_chat" ? t("toast.openaiFormatExperimental") : t("toast.anthropicFormatSelected"));
        return;
      }
      const presetButton = event.target.closest("[data-preset]");
      if (presetButton && presetButton.closest("#presetList")) {
        event.preventDefault();
        await fillPreset(presetButton.dataset.preset);
        return;
      }
      const presetModelOption = event.target.closest("[data-preset-model-option]");
      if (presetModelOption) {
        applyPresetModelOption(presetModelOption.dataset.presetModelOption, presetModelOption.checked);
        return;
      }
      await handleAction(event.target);
    });

    document.addEventListener("change", (event) => {
      const mappingInput = event.target.closest("[data-provider-model-input]");
      if (mappingInput) {
        updateProviderModelInput(mappingInput.dataset.providerModelInput, mappingInput.value);
        renderPresetOptions(selectedPreset, collectProviderMappings());
      }
      if (event.target.id === "providerBaseUrl") {
        renderBaseUrlOptions();
      }
    });

    document.addEventListener("input", (event) => {
      if (event.target.id === "providerBaseUrl") {
        renderBaseUrlOptions();
        protocolDetected = false;
        updateDetectFormatButton();
        updateApplyButtonState();
      }
      if (event.target.id === "providerApiKey") {
        updateDetectFormatButton();
      }
      const mappingInput = event.target.closest("[data-provider-model-input]");
      if (!mappingInput) return;
      updateProviderModelInput(mappingInput.dataset.providerModelInput, mappingInput.value);
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        closeBaseUrlMenu();
        closeProviderSlotMenu();
        closeProviderModelMenu();
        closeAuthSchemeMenu();
      }
    });

    $("#providerForm").addEventListener("submit", async (event) => {
      event.preventDefault();
      try {
        const wasEditing = !!editingProviderId;
        await saveProviderFromForm();
        if (editingProviderId) {
          showToast(wasEditing ? t("toast.providerUpdated") : t("toast.providerSaved"));
        } else {
          showToast(t("toast.providerSaved"));
        }
        editingProviderId = null;
        selectedPreset = null;
        window.location.hash = "providers";
      } catch (error) {
        console.error(error);
        showToast(error.message || t("toast.requestFailed"));
      }
    });

    $("#modelProvider")?.addEventListener("change", renderMappingCards);
    $("#settingsProxyPort").addEventListener("change", saveSettingsFromForm);
    $("#settingsAdminPort").addEventListener("change", saveSettingsFromForm);
    $("#settingsUpdateUrl").addEventListener("change", saveSettingsFromForm);
    $("#settingsUpstreamProxy").addEventListener("change", saveSettingsFromForm);
    $("#autoStart").addEventListener("change", saveSettingsFromForm);
    $("#exposeAllProviderModels").addEventListener("change", saveSettingsFromForm);
    $("#configImportFile")?.addEventListener("change", (event) => {
      importConfigFile(event.target.files?.[0]);
    });
    $("#restartReminderAck")?.addEventListener("click", dismissRestartReminder);
    $("#restartReminderNow")?.addEventListener("click", async (event) => {
      try {
        await restartClaudeDesktopFromUi(event.currentTarget, true);
      } catch (error) {
        console.error(error);
        showToast(error.message || t("toast.requestFailed"));
      }
    });

    $("#confirmDelete").addEventListener("click", async () => {
      if (!pendingDeleteId) return;
      try {
        await CCApi.deleteProvider(pendingDeleteId);
        pendingDeleteId = null;
        deleteModal.hide();
        if (routeFromHash() === "dashboard") {
          await renderDashboard();
        } else {
          await renderProviders();
        }
        showToast(t("toast.providerDeleted"));
      } catch (error) {
        console.error(error);
        showToast(error.message || t("toast.requestFailed"));
      }
    });
  }

  document.addEventListener("DOMContentLoaded", async () => {
    deleteModal = new bootstrap.Modal($("#deleteModal"));
    confirmModal = new bootstrap.Modal($("#confirmModal"));
    restartReminderModal = new bootstrap.Modal($("#restartReminderModal"), {
      backdrop: "static",
      keyboard: false,
    });
    toast = new bootstrap.Toast($("#appToast"), { delay: 2200 });
    bindEvents();
    bindFeedbackEvents();
    const settings = await CCApi.getSettings();
    CCI18n.apply(settings.language || "zh");
    applyTheme(settings.theme || "default");
    if (!window.location.hash) window.location.hash = "dashboard";
    await renderRoute(routeFromHash());
  });
