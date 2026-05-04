const FRONTEND_ROOT = './';

function invoke(command, args = {}) {
  const coreInvoke = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
  if (!coreInvoke) {
    throw new Error('Tauri invoke API is unavailable');
  }
  return coreInvoke(command, args);
}

function asError(error) {
  return error instanceof Error ? error : new Error(String(error));
}

async function call(command, args = {}) {
  try {
    return await invoke(command, args);
  } catch (error) {
    throw asError(error);
  }
}

function apiFormat(value) {
  return ['OpenAI', 'openai', 'openai_chat'].includes(value) ? 'openai_chat' : 'anthropic';
}

function uiApiFormat(value) {
  return ['openai', 'openai_chat'].includes(value) ? 'OpenAI' : 'Anthropic';
}

function providerBody(payload = {}, includeModels = true) {
  const body = {
    id: payload.id,
    name: payload.name,
    baseUrl: payload.baseUrl,
    authScheme: payload.authScheme || 'bearer',
    apiFormat: apiFormat(payload.apiFormat),
    extraHeaders: payload.extraHeaders || {},
    modelCapabilities: payload.modelCapabilities || {},
    requestOptions: payload.requestOptions || {},
  };
  if (payload.apiKey) body.apiKey = payload.apiKey;
  if (includeModels) body.models = payload.models || {};
  return body;
}

function iconForProvider(provider = {}) {
  const id = `${provider.id || ''} ${provider.name || ''} ${provider.baseUrl || ''}`.toLowerCase();
  const logoMap = [
    ['deepseek', 'assets/providers/deepseek.ico'],
    ['kimi', 'assets/providers/kimi.ico'],
    ['moonshot', 'assets/providers/kimi.ico'],
    ['xiaomi', 'assets/providers/xiaomi-mimo.png'],
    ['mimo', 'assets/providers/xiaomi-mimo.png'],
    ['qiniu', 'assets/providers/qiniu.ico'],
    ['qnaigc', 'assets/providers/qiniu.ico'],
    ['zhipu', 'assets/providers/zhipu.png'],
    ['bigmodel', 'assets/providers/zhipu.png'],
    ['glm', 'assets/providers/zhipu.png'],
    ['bailian', 'assets/providers/aliyun.ico'],
    ['dashscope', 'assets/providers/aliyun.ico'],
    ['aliyun', 'assets/providers/aliyun.ico'],
  ];
  for (const [key, logo] of logoMap) {
    if (id.includes(key)) return { logo: `${FRONTEND_ROOT}${logo}` };
  }
  if (id.includes('siliconflow')) return { icon: 'bi-diagram-3-fill' };
  return { icon: 'bi-plug-fill' };
}

function mapProvider(provider = {}, activeId = null) {
  const models = provider.models || {};
  return {
    ...provider,
    id: provider.id,
    name: provider.name,
    baseUrl: provider.baseUrl,
    apiFormat: apiFormat(provider.apiFormat),
    authScheme: provider.authScheme || 'bearer',
    hasApiKey: !!provider.hasApiKey,
    extraHeaders: provider.extraHeaders || {},
    modelCapabilities: provider.modelCapabilities || {},
    requestOptions: provider.requestOptions || {},
    default: provider.id === activeId,
    isBuiltin: !!provider.isBuiltin,
    mappings: {
      default: models.default || '',
      opus_4_7: models.opus_4_7 || models.opus || '',
      opus_4_6: models.opus_4_6 || '',
      opus_3: models.opus_3 || '',
      sonnet_4_6: models.sonnet_4_6 || models.sonnet || '',
      sonnet_4_5: models.sonnet_4_5 || '',
      haiku_4_5: models.haiku_4_5 || models.haiku || '',
    },
    ...iconForProvider(provider),
  };
}

function mapPreset(preset = {}) {
  return {
    ...preset,
    apiFormat: uiApiFormat(preset.apiFormat),
    authScheme: preset.authScheme || 'bearer',
    models: preset.models || {},
    modelOptions: preset.modelOptions || {},
    baseUrlOptions: preset.baseUrlOptions || [],
    baseUrlHint: preset.baseUrlHint || '',
    requestOptionPresets: preset.requestOptionPresets || {},
    extraHeaders: preset.extraHeaders || {},
    modelCapabilities: preset.modelCapabilities || {},
    requestOptions: preset.requestOptions || {},
    ...iconForProvider(preset),
  };
}

function mapLog(log = {}) {
  return {
    at: log.time,
    level: String(log.level || '').toLowerCase(),
    message: log.message || '',
  };
}

function successMessage(success, fallback) {
  return success?.message || fallback;
}

async function configSnapshot() {
  return call('get_config_snapshot');
}

async function currentProviders() {
  const config = await configSnapshot();
  return (config.providers || []).map((provider) => mapProvider(provider, config.activeProvider));
}

async function currentProvider(id) {
  const config = await configSnapshot();
  return (config.providers || []).find((provider) => provider.id === id) || null;
}

async function getStatus() {
  const [config, desktop, desktopHealth, proxy] = await Promise.all([
    configSnapshot(),
    call('get_desktop_status'),
    call('get_desktop_health'),
    call('get_proxy_status'),
  ]);
  const active = (config.providers || []).find((provider) => provider.id === config.activeProvider);
  return {
    desktopConfigured: !!desktop.configured,
    proxyRunning: !!proxy.running,
    proxyPort: proxy.port || config.settings?.proxyPort || 18080,
    activeProvider: active ? { name: active.name, id: active.id } : { name: '-', id: null },
    activeProviderId: config.activeProvider,
    desktopHealth: desktopHealth || { needsApply: false, issues: [] },
    exposeAllProviderModels: !!config.settings?.exposeAllProviderModels,
  };
}

async function getDesktopStatus() {
  const [desktop, health, settings] = await Promise.all([
    call('get_desktop_status'),
    call('get_desktop_health'),
    call('get_settings'),
  ]);
  const keys = desktop.keys || {};
  return {
    configured: !!desktop.configured,
    health: health || { needsApply: false, issues: [] },
    config: {
      inferenceProvider: keys.inferenceProvider || 'gateway',
      inferenceGatewayBaseUrl: keys.inferenceGatewayBaseUrl || `http://127.0.0.1:${settings.proxyPort || 18080}`,
      inferenceGatewayApiKey: keys.inferenceGatewayApiKey ? '******' : '',
      inferenceGatewayAuthScheme: keys.inferenceGatewayAuthScheme || 'bearer',
      inferenceModels: keys.inferenceModels || '["sonnet","haiku","opus"]',
    },
  };
}

async function detectApiFormat(baseUrl, apiKey = '') {
  return call('detect_api_format', { baseUrl, apiKey });
}

async function checkModelAvailability(providerId, model) {
  return call('check_model_available', { providerId, model });
}

window.CCApi = {
  getStatus,

  async getProviders() {
    return currentProviders();
  },

  async getProviderSecret(id) {
    return call('get_provider_secret', { providerId: id });
  },

  async getPresets() {
    const presets = await call('list_builtin_presets');
    return (presets || []).map(mapPreset);
  },

  async addProvider(payload) {
    return call('add_provider', { provider: providerBody(payload) });
  },

  async updateProvider(id, payload) {
    const provider = await call('update_provider', { providerId: id, provider: providerBody(payload) });
    if (!provider) throw new Error('提供商不存在');
    return provider;
  },

  async deleteProvider(id) {
    const success = await call('delete_provider', { providerId: id });
    if (!success) throw new Error('提供商不存在');
    return { success: true, message: '已删除' };
  },

  detectApiFormat,

  async setDefaultProvider(id) {
    const provider = await call('set_active_provider', { providerId: id });
    if (!provider) throw new Error('提供商不存在');
    let desktopSync = null;
    try {
      desktopSync = { attempted: true, ...(await call('configure_desktop')) };
    } catch (error) {
      desktopSync = {
        attempted: true,
        success: false,
        message: `桌面版模型同步失败: ${asError(error).message}`,
      };
    }
    return { success: true, message: '默认提供商已更新', desktopSync };
  },

  async reorderProviders(providerIds) {
    const success = await call('reorder_providers', { providerIds });
    return { success };
  },

  async testProvider(id) {
    return call('test_saved_provider', { providerId: id });
  },

  async queryProviderUsage(id) {
    return call('query_provider_usage', { providerId: id });
  },

  async getProviderCompatibility() {
    return call('provider_compatibility_report');
  },

  async testProviderPayload(payload) {
    return call('test_provider', { provider: providerBody(payload, false) });
  },

  async saveModelMappings(id, mappings) {
    const success = await call('update_provider_models', { providerId: id, models: mappings });
    if (!success) throw new Error('提供商不存在');
    return { success: true, message: '模型映射已保存' };
  },

  async fetchProviderModels(id) {
    return call('fetch_saved_provider_models', { providerId: id });
  },

  async fetchProviderModelsPayload(payload) {
    return call('fetch_provider_models', { provider: providerBody(payload, false) });
  },

  async autofillProviderModels(id) {
    return call('autofill_provider_models', { providerId: id });
  },

  checkModelAvailability,

  getDesktopStatus,

  async configureDesktop() {
    const applyResult = await call('configure_desktop');
    const status = await getDesktopStatus();
    return { ...status, ...applyResult };
  },

  async clearDesktop() {
    await call('clear_desktop_config');
    return getDesktopStatus();
  },

  async restartClaudeDesktop() {
    return call('restart_claude_desktop');
  },

  async startProxy(port) {
    if (port) await this.saveSettings({ proxyPort: Number(port) });
    const status = await call('start_proxy_listener');
    return { running: !!status.running, port: status.port || Number(port) || 18080 };
  },

  async stopProxy() {
    const status = await call('stop_proxy_listener');
    return { running: !!status.running, port: status.port || 18080 };
  },

  async getProxyLogs() {
    const logs = await call('get_proxy_logs');
    return (logs || []).map(mapLog);
  },

  async getProxyStatus() {
    const data = await call('get_proxy_status');
    return {
      running: !!data.running,
      port: data.port || 18080,
      stats: data.stats || { total: 0, success: 0, failed: 0, today: 0 },
    };
  },

  async clearLogs() {
    return call('clear_proxy_logs');
  },

  async getSettings() {
    return call('get_settings');
  },

  async saveSettings(settings) {
    return call('update_settings', { settings });
  },

  async detectLocalProxy() {
    return call('detect_local_proxy');
  },

  async checkUpdate(updateUrl) {
    return call('check_update', { url: updateUrl || undefined });
  },

  async installUpdate(updateUrl) {
    return call('install_update', { url: updateUrl || undefined });
  },

  async getUpdateProgress() {
    return call('get_update_progress');
  },

  async createBackup() {
    const backup = await call('create_config_backup', { reason: 'manual' });
    return { success: true, backup };
  },

  async listBackups() {
    return call('list_config_backups');
  },

  async exportConfig() {
    return call('export_config');
  },

  async importConfig(configData) {
    return call('import_config', { config: configData });
  },

  async submitFeedback(payload) {
    return call('submit_feedback', { payload });
  },

  async getCcSwitchStatus() {
    return call('get_ccswitch_status');
  },

  async getCcSwitchProviders() {
    return call('get_ccswitch_providers');
  },

  async importCcSwitchProviders(ids, setDefault = false) {
    return call('import_ccswitch_providers', { ids, setDefault });
  },

  async getActivities() {
    const logs = await call('get_proxy_logs');
    return (logs || []).slice(-5).reverse().map((log) => ({
      time: log.time,
      text: log.message,
    }));
  },

  async getModels(id) {
    const provider = await currentProvider(id);
    if (!provider) throw new Error('提供商不存在');
    return { models: provider.models || {} };
  },
};
