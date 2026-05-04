  const routes = ["dashboard", "providers/add", "providers", "desktop", "proxy", "settings", "guide"];
  const restartReminderStorageKey = "ccds.restartReminder.dismissed";
  const modelMeta = [
    { key: "sonnet", title: "Sonnet", icon: "bi-stars", source: "claude-sonnet-4-6" },
    { key: "haiku", title: "Haiku", icon: "bi-leaf", source: "claude-haiku-3-5" },
    { key: "opus", title: "Opus", icon: "bi-box", source: "claude-opus-4-7" },
  ];
  const providerFormModelSlots = [
    { key: "default", label: "Default", icon: "bi-circle-fill", iconClass: "default", source: "未配置映射时默认使用这一项", required: true },
    { key: "opus_4_7", label: "Opus 4.7", icon: "bi-box", iconClass: "opus", source: "claude-opus-4-7" },
    { key: "opus_4_6", label: "Opus 4.6", icon: "bi-box", iconClass: "opus", source: "claude-opus-4-6" },
    { key: "opus_3", label: "Opus 3", icon: "bi-box", iconClass: "opus", source: "claude-3-opus" },
    { key: "sonnet_4_6", label: "Sonnet 4.6", icon: "bi-stars", iconClass: "sonnet", source: "claude-sonnet-4-6" },
    { key: "sonnet_4_5", label: "Sonnet 4.5", icon: "bi-stars", iconClass: "sonnet", source: "claude-sonnet-4-5" },
    { key: "haiku_4_5", label: "Haiku 4.5", icon: "bi-leaf", iconClass: "haiku", source: "claude-haiku-4-5" },
  ];
  const availableThemes = ["default", "green", "orange", "gray", "dark", "white"];
  const providerAuthSchemes = ["bearer", "x-api-key", "none"];
  const providerFormDefaultRows = ["default", "opus_4_7", "sonnet_4_6", "haiku_4_5"];
  let pendingDeleteId = null;
  let selectedPreset = null;
  let presetCache = [];
  let formApiFormat = "Anthropic";
  let formModelCapabilities = {};
  let formRequestOptions = {};
  let providerFormMappings = {};
  let providerFormRows = [...providerFormDefaultRows];
  let providerAvailableModels = [];
  let openProviderSlotMenuIndex = null;
  let openProviderModelMenuKey = null;
  let protocolDetected = false;
  let baseUrlMenuOpen = false;
  let authSchemeMenuOpen = false;
  let editingProviderId = null;
  let deleteModal = null;
  let confirmModal = null;
  let restartReminderModal = null;
  let feedbackBsModal = null;
  let feedbackAttachments = [];
  let toast = null;
  let updateCheckCache = null;
  let updateInstallPhase = "idle";
  let ccSwitchCandidates = [];
  let proxyLogTimer = null;
  let proxyLogInflight = false;
