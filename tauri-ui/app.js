const tauri = window.__TAURI__;

if (!tauri?.core?.invoke) {
  document.body.innerHTML =
    "<main style='padding:24px;color:white;font-family:Segoe UI,sans-serif'>No se encontro la API de Tauri. Ejecuta el launcher con <code>npm run dev</code> o <code>tauri dev</code>.</main>";
  throw new Error("Tauri API unavailable");
}

const { invoke } = tauri.core;
const { listen } = tauri.event;

const state = {
  bootstrap: null,
  bootstrapFingerprint: "",
  selectedInstanceKey: null,
  settingsOpen: false,
  activeSettingsTab: "account",
  accountAvatarRequestId: 0,
  activeNewsIndex: 0,
  gameLifecycleStatus: "idle",
  currentGameInstanceKey: null,
  backendPollInFlight: false,
  lastBackendPollAt: 0,
  notificationsOpen: false,
  notificationsUnread: 0,
  notifications: []
};

const BACKEND_POLL_INTERVAL_MS = 10000;
const BACKEND_POLL_DEBOUNCE_MS = 3000;

const els = {
  loginView: document.querySelector("#login-view"),
  dashboardView: document.querySelector("#dashboard-view"),
  loginBrandText: document.querySelector("#login-brand-text"),
  windowBrandText: document.querySelector("#window-brand-text"),
  notificationsBtn: document.querySelector("#notifications-btn"),
  notificationBadge: document.querySelector("#notification-badge"),
  notificationsPanel: document.querySelector("#notifications-panel"),
  notificationsList: document.querySelector("#notifications-list"),
  clearNotificationsBtn: document.querySelector("#clear-notifications-btn"),
  loginStatusTitle: document.querySelector("#login-status-title"),
  loginStatusDetail: document.querySelector("#login-status-detail"),
  statusTitle: document.querySelector("#status-title"),
  statusDetail: document.querySelector("#status-detail"),
  statusProgress: document.querySelector("#status-progress"),
  statusProgressFill: document.querySelector("#status-progress-fill"),
  statusProgressValue: document.querySelector("#status-progress-value"),
  offlineUsername: document.querySelector("#offline-username"),
  offlineLoginBtn: document.querySelector("#offline-login-btn"),
  microsoftLoginBtn: document.querySelector("#microsoft-login-btn"),
  deviceBox: document.querySelector("#device-box"),
  deviceMessage: document.querySelector("#device-message"),
  deviceCode: document.querySelector("#device-code"),
  deviceLink: document.querySelector("#device-link"),
  instancesList: document.querySelector("#instances-list"),
  instanceTemplate: document.querySelector("#instance-template"),
  selectedServerLabel: document.querySelector("#selected-server-label"),
  selectedInstanceTitle: document.querySelector("#selected-instance-title"),
  selectedLoaderCopy: document.querySelector("#selected-loader-copy"),
  sourceChip: document.querySelector("#source-chip"),
  serverChip: document.querySelector("#server-chip"),
  heroStage: document.querySelector("#hero-stage"),
  heroImage: document.querySelector("#hero-image"),
  playBtn: document.querySelector("#play-btn"),
  closeGameBtn: document.querySelector("#close-game-btn"),
  logoutBtn: document.querySelector("#logout-btn"),
  newsPanel: document.querySelector("#news-panel"),
  newsCount: document.querySelector("#news-count"),
  newsTitle: document.querySelector("#news-title"),
  newsMeta: document.querySelector("#news-meta"),
  newsContent: document.querySelector("#news-content"),
  newsList: document.querySelector("#news-list"),
  settingsLayer: document.querySelector("#settings-layer"),
  settingsBackdrop: document.querySelector("#settings-backdrop"),
  closeSettingsBtn: document.querySelector("#close-settings-btn"),
  saveSettingsBtn: document.querySelector("#save-settings-btn"),
  settingsBackBtn: document.querySelector("#settings-back-btn"),
  toggleSettingsBtn: document.querySelector("#toggle-settings-btn"),
  settingsNavButtons: Array.from(document.querySelectorAll("[data-settings-tab]")),
  settingsPanels: Array.from(document.querySelectorAll("[data-settings-panel]")),
  launcherSettingsTabButton: document.querySelector('[data-settings-tab="launcher"]'),
  launcherSettingsPanel: document.querySelector('[data-settings-panel="launcher"]'),
  controlPanel: document.querySelector("#control-panel"),
  accountChip: document.querySelector("#account-chip"),
  accountAvatar: document.querySelector("#account-avatar"),
  accountAvatarImage: document.querySelector("#account-avatar-image"),
  accountAvatarFallback: document.querySelector("#account-avatar-fallback"),
  settingsAccountAvatarImage: document.querySelector("#settings-account-avatar-image"),
  settingsAccountAvatarFallback: document.querySelector("#settings-account-avatar-fallback"),
  settingsAccountName: document.querySelector("#settings-account-name"),
  settingsAccountType: document.querySelector("#settings-account-type"),
  settingsAccountUuid: document.querySelector("#settings-account-uuid"),
  settingsAccountNote: document.querySelector("#settings-account-note"),
  settingsLogoutBtn: document.querySelector("#settings-logout-btn"),
  accountSummary: document.querySelector("#account-summary"),
  backendLocalPath: document.querySelector("#backend-local-path"),
  backendLocalField: document.querySelector("#backend-local-path")?.closest(".field"),
  backendBaseUrl: document.querySelector("#backend-base-url"),
  preferLocalBackend: document.querySelector("#prefer-local-backend"),
  preferLocalBackendRow:
    document.querySelector("#prefer-local-backend")?.closest(".toggle-row"),
  updaterEndpoint: document.querySelector("#updater-endpoint"),
  updaterPublicKey: document.querySelector("#updater-public-key"),
  minMemory: document.querySelector("#min-memory"),
  maxMemory: document.querySelector("#max-memory"),
  settingsSelectedInstance: document.querySelector("#settings-selected-instance"),
  settingsSelectedLoader: document.querySelector("#settings-selected-loader"),
  settingsSelectedServer: document.querySelector("#settings-selected-server"),
  settingsBackendSummary: document.querySelector("#settings-backend-summary"),
  settingsSourceMode: document.querySelector("#settings-source-mode"),
  settingsInstanceCount: document.querySelector("#settings-instance-count"),
  settingsAppVersion: document.querySelector("#settings-app-version"),
  settingsInfoAccount: document.querySelector("#settings-info-account"),
  settingsInfoInstance: document.querySelector("#settings-info-instance")
};

const applyAdaptiveScale = () => {
  const width = window.innerWidth || 0;
  const height = window.innerHeight || 0;
  const dpiScale = window.devicePixelRatio || 1;

  let scale = 1;

  if (width > 980) {
    const widthScale = width / 1240;
    const heightScale = height / 800;
    scale = Math.min(1, widthScale, heightScale);

    if (dpiScale >= 1.5) {
      scale = Math.min(scale, 0.94);
    } else if (dpiScale >= 1.25) {
      scale = Math.min(scale, 0.97);
    }

    scale = Math.max(scale, 0.82);
  }

  document.documentElement.style.setProperty("--app-scale", scale.toFixed(4));
  document.documentElement.style.setProperty(
    "--app-unscale",
    (1 / scale).toFixed(4)
  );
  document.body.classList.toggle("is-app-scaled", scale < 0.999);
};

const setStatus = (title, detail) => {
  els.loginStatusTitle.textContent = title;
  els.loginStatusDetail.textContent = detail;
  els.statusTitle.textContent = title;
  els.statusDetail.textContent = detail;
};

const formatNotificationTimestamp = (timestamp) => {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return "Ahora";
  }

  return date.toLocaleTimeString("es-MX", {
    hour: "2-digit",
    minute: "2-digit"
  });
};

const renderNotifications = () => {
  const unread = state.notificationsUnread;
  els.notificationBadge.textContent = unread > 9 ? "9+" : String(unread);
  els.notificationBadge.classList.toggle("is-hidden", unread <= 0);
  els.notificationsPanel.classList.toggle("is-open", state.notificationsOpen);
  els.notificationsPanel.setAttribute("aria-hidden", String(!state.notificationsOpen));
  els.notificationsBtn.setAttribute("aria-expanded", String(state.notificationsOpen));
  els.notificationsList.innerHTML = "";

  if (!state.notifications.length) {
    const empty = document.createElement("div");
    empty.className = "notifications-panel__empty";
    empty.textContent = "Sin notificaciones por ahora.";
    els.notificationsList.appendChild(empty);
    return;
  }

  state.notifications.forEach((notification) => {
    const article = document.createElement("article");
    const title = document.createElement("strong");
    const detail = document.createElement("p");
    const time = document.createElement("time");

    article.className = "notifications-panel__item";
    title.textContent = notification.title;
    detail.textContent = notification.detail;
    time.textContent = formatNotificationTimestamp(notification.timestamp);

    article.append(title, detail, time);
    els.notificationsList.appendChild(article);
  });
};

const setNotificationsOpen = (open) => {
  state.notificationsOpen = open;

  if (open) {
    state.notificationsUnread = 0;
  }

  renderNotifications();
};

const pushNotification = (title, detail) => {
  const previous = state.notifications[0];
  if (
    previous &&
    previous.title === title &&
    previous.detail === detail &&
    Date.now() - previous.timestamp < 15000
  ) {
    return;
  }

  state.notifications.unshift({
    id: crypto.randomUUID(),
    title,
    detail,
    timestamp: Date.now()
  });
  state.notifications = state.notifications.slice(0, 10);

  if (!state.notificationsOpen) {
    state.notificationsUnread += 1;
  }

  renderNotifications();
};

const clampProgress = (value) => {
  const numeric = Number(value);
  if (Number.isNaN(numeric)) return 0;
  return Math.max(0, Math.min(1, numeric));
};

const setStatusProgress = (progress, visible) => {
  const safeProgress = clampProgress(progress);
  els.statusProgress.classList.toggle("is-hidden", !visible);
  els.statusProgress.setAttribute("aria-hidden", String(!visible));
  els.statusProgressFill.style.width = `${safeProgress * 100}%`;
  els.statusProgressValue.textContent = `${Math.round(safeProgress * 100)}%`;
};

const resetGameLifecycle = () => {
  state.gameLifecycleStatus = "idle";
  state.currentGameInstanceKey = null;
  els.playBtn.disabled = !selectedInstance();
  els.playBtn.textContent = "Play";
  els.closeGameBtn.classList.add("is-hidden");
  els.closeGameBtn.disabled = true;
  setStatusProgress(0, false);
};

const applyGameLifecycle = (payload) => {
  if (payload.instanceKey && state.selectedInstanceKey && payload.instanceKey !== state.selectedInstanceKey) {
    state.selectedInstanceKey = payload.instanceKey;
    renderInstances(state.bootstrap?.instances || []);
  }

  state.gameLifecycleStatus = payload.status || "idle";
  state.currentGameInstanceKey = payload.instanceKey || state.currentGameInstanceKey;

  setStatus(payload.stage || "Lanzando", payload.detail || "Preparando cliente.");
  setStatusProgress(payload.progress ?? 0, payload.progress != null);

  const isRunning = payload.status === "running";
  const isLaunching = payload.status === "launching" || payload.status === "closing";
  const canClose = Boolean(payload.canClose) && (isRunning || isLaunching);

  els.playBtn.disabled = isRunning || isLaunching || !selectedInstance();
  els.playBtn.textContent = isRunning
    ? "Juego abierto"
    : isLaunching
      ? "Abriendo..."
      : "Play";

  els.closeGameBtn.classList.toggle("is-hidden", !canClose);
  els.closeGameBtn.disabled = !canClose;

  if (payload.status === "stopped" || payload.status === "error") {
    state.gameLifecycleStatus = "idle";
    state.currentGameInstanceKey = null;
    els.playBtn.disabled = !selectedInstance();
    els.playBtn.textContent = "Play";
    els.closeGameBtn.classList.add("is-hidden");
    els.closeGameBtn.disabled = true;
  }
};

const applySyncProgress = (payload) => {
  const total = Math.max(1, Number(payload.total) || 1);
  const current = Math.max(0, Math.min(total, Number(payload.current) || 0));
  const progress = 0.08 + (current / total) * 0.64;

  state.gameLifecycleStatus = "launching";
  setStatus(
    `Sincronizando ${current}/${total}`,
    payload.file || "Procesando archivos del cliente."
  );
  setStatusProgress(progress, true);
  els.playBtn.disabled = true;
  els.playBtn.textContent = "Abriendo...";
  els.closeGameBtn.classList.add("is-hidden");
  els.closeGameBtn.disabled = true;
};

const applyLauncherStage = (payload) => {
  setStatus(payload.stage, payload.detail);

  if (state.gameLifecycleStatus === "running" || state.gameLifecycleStatus === "closing") {
    return;
  }

  const stage = String(payload.stage || "").toLowerCase();
  let progress = null;

  if (stage.includes("sincronizando")) {
    progress = 0.18;
  } else if (
    stage.includes("runtime protegido") ||
    stage.includes("backend local") ||
    stage.includes("preparando cliente")
  ) {
    progress = 0.74;
  } else if (stage.includes("lanzando")) {
    progress = 0.82;
  } else if (stage.includes("reintentando")) {
    progress = 0.8;
  } else if (stage.includes("backend ocupado")) {
    progress = 0.78;
  }

  if (progress != null) {
    state.gameLifecycleStatus = "launching";
    setStatusProgress(progress, true);
    els.playBtn.disabled = true;
    els.playBtn.textContent = "Abriendo...";
  }
};

const normalizeErrorMessage = (error, fallback = "Ocurrio un error del cliente.") => {
  const raw = String(error || "").trim();

  if (isStaffSession()) {
    return raw || fallback;
  }

  const lowered = raw.toLowerCase();

  if (lowered.includes("mantenimiento")) {
    return raw;
  }

  if (lowered.includes("staff")) {
    return raw;
  }

  if (lowered.includes("inicia sesion")) {
    return "Inicia sesion antes de continuar.";
  }

  if (lowered.includes("client_id")) {
    return "No fue posible preparar el inicio de sesion con Microsoft.";
  }

  if (
    lowered.includes("cancelado") ||
    lowered.includes("minecraft java") ||
    lowered.includes("xbox live") ||
    lowered.includes("codigo de autorizacion")
  ) {
    return raw || fallback;
  }

  if (lowered.includes("premium")) {
    return "No fue posible completar el inicio de sesion premium.";
  }

  if (lowered.includes("microsoft")) {
    return "No fue posible completar el inicio de sesion con Microsoft.";
  }

  if (lowered.includes("offline")) {
    return "No fue posible crear la sesion local.";
  }

  return fallback;
};

const setErrorStatus = (error, fallback) => {
  setStatus("Error", normalizeErrorMessage(error, fallback));
};

const setView = (view) => {
  const showDashboard = view === "dashboard";
  els.loginView.classList.toggle("view--active", !showDashboard);
  els.dashboardView.classList.toggle("view--active", showDashboard);
};

const applyAppMeta = (bootstrap) => {
  const productName = bootstrap?.productName || "Tavari Client";
  const appVersion = bootstrap?.appVersion || "";
  const label = appVersion ? `${productName} ${appVersion}` : productName;

  document.title = productName;
  els.loginBrandText.textContent = label;
  els.windowBrandText.textContent = label;
  els.settingsAppVersion.textContent = label;
};

const humanAccountType = (account) => {
  if (!account) return "Sin sesion";
  return account.kind === "microsoft" ? "Premium Microsoft" : "No premium";
};

const initialsFromText = (value) =>
  (value || "?")
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() || "")
    .join("") || "?";

const skinPreviewUrl = (account) => {
  if (!account) return "";
  const identity =
    account.kind === "microsoft" && account.uuid ? account.uuid : account.username;
  return `https://mc-heads.net/avatar/${encodeURIComponent(identity)}/128`;
};

const renderAccountAvatar = (account) => {
  const fallback = account ? initialsFromText(account.username) : "?";
  const requestId = state.accountAvatarRequestId + 1;
  const avatarTargets = [
    [els.accountAvatarImage, els.accountAvatarFallback],
    [els.settingsAccountAvatarImage, els.settingsAccountAvatarFallback]
  ];

  state.accountAvatarRequestId = requestId;

  avatarTargets.forEach(([image, fallbackNode]) => {
    fallbackNode.textContent = fallback;
    image.classList.remove("is-visible");
    image.removeAttribute("src");
  });

  els.accountAvatar.dataset.mode = "fallback";

  if (!account) {
    return;
  }

  const previewUrl = skinPreviewUrl(account);
  const preview = new Image();

  preview.onload = () => {
    if (state.accountAvatarRequestId !== requestId) return;

    avatarTargets.forEach(([image]) => {
      image.src = previewUrl;
      image.classList.add("is-visible");
    });

    els.accountAvatar.dataset.mode = "skin";
  };

  preview.onerror = () => {
    if (state.accountAvatarRequestId !== requestId) return;

    avatarTargets.forEach(([image]) => {
      image.classList.remove("is-visible");
    });

    els.accountAvatar.dataset.mode = "fallback";
  };

  preview.src = previewUrl;
};

const visibleInstances = (instances = []) =>
  instances.filter((instance) => !instance.hidden);

const selectedInstance = () =>
  state.bootstrap?.instances?.find((item) => item.key === state.selectedInstanceKey) || null;

const newsItems = () => state.bootstrap?.news || [];

const canAccessLauncherSettings = () => Boolean(state.bootstrap?.isStaff);
const isStaffSession = () => Boolean(state.bootstrap?.isStaff);
const canUseLocalBackend = () => Boolean(state.bootstrap?.isDebugBuild);
const safeServerText = (instance) => {
  if (!instance) return "sin servidor";
  return instance.serverLabel || (instance.serverAddress ? "Servidor privado" : "sin servidor");
};

const setActiveSettingsTab = (tab) => {
  const nextTab =
    tab === "launcher" && !canAccessLauncherSettings() ? "account" : tab;

  state.activeSettingsTab = nextTab;

  els.settingsNavButtons.forEach((button) => {
    button.classList.toggle("is-active", button.dataset.settingsTab === nextTab);
  });

  els.settingsPanels.forEach((panel) => {
    panel.classList.toggle("is-active", panel.dataset.settingsPanel === nextTab);
  });
};

const applyStaffVisibility = (bootstrap) => {
  const canOpenLauncherTab = Boolean(bootstrap?.isStaff);

  if (els.launcherSettingsTabButton) {
    els.launcherSettingsTabButton.hidden = !canOpenLauncherTab;
  }

  if (els.launcherSettingsPanel) {
    els.launcherSettingsPanel.hidden = !canOpenLauncherTab;
  }

  if (!canOpenLauncherTab && state.activeSettingsTab === "launcher") {
    state.activeSettingsTab = "account";
  }
};

const setSettingsOpen = (open) => {
  state.settingsOpen = open;
  els.heroStage.classList.toggle("is-settings-open", open);
  els.settingsLayer.classList.toggle("is-open", open);
  els.toggleSettingsBtn.classList.toggle("is-active", open);
  els.toggleSettingsBtn.setAttribute("aria-expanded", String(open));
  els.settingsLayer.setAttribute("aria-hidden", String(!open));
};

const renderSettingsContext = () => {
  const instance = selectedInstance();
  const account = state.bootstrap?.account || null;

  els.settingsSelectedInstance.textContent = instance?.name || "-";
  els.settingsSelectedLoader.textContent = instance
    ? `${instance.loaderType} ${instance.loaderVersion} / MC ${instance.minecraftVersion}`
    : "-";
  els.settingsSelectedServer.textContent = safeServerText(instance);
  els.settingsBackendSummary.textContent =
    state.bootstrap?.backendSummary || "Sin datos del backend.";
  els.settingsSourceMode.textContent = instance?.sourceMode || "sin backend";
  els.settingsInstanceCount.textContent = String(
    visibleInstances(state.bootstrap?.instances || []).length
  );
  els.settingsInfoAccount.textContent = account ? account.username : "Sin sesion";
  els.settingsInfoInstance.textContent = instance?.name || "-";
};

const performLogout = async () => {
  await invoke("logout");
  setSettingsOpen(false);
  await refreshBootstrap();
  setView("login");
};

const renderAccount = (account) => {
  els.accountChip.textContent = account
    ? `${account.username} - ${humanAccountType(account)}`
    : "Sin sesion";
  renderAccountAvatar(account);
  els.settingsAccountName.textContent = account ? account.username : "Sin sesion";
  els.settingsAccountType.textContent = account
    ? humanAccountType(account)
    : "No autenticado";
  els.settingsAccountUuid.textContent = `UUID: ${account?.uuid || "-"}`;
  els.settingsAccountNote.textContent = account
    ? `Sesion lista para jugar con ${account.username}.`
    : "Inicia sesion con Microsoft o usa una cuenta no premium para jugar.";
  els.settingsLogoutBtn.disabled = !account;

  els.accountSummary.innerHTML = "";
  const rows = account
    ? [
        ["Usuario", account.username],
        ["Tipo", humanAccountType(account)],
        ["UUID", account.uuid],
        ["Ultima sesion", account.lastUsedAt || "Ahora"]
      ]
    : [["Sesion", "No iniciada"]];

  rows.forEach(([label, value]) => {
    const row = document.createElement("div");
    row.className = "meta-row";
    row.innerHTML = `<span>${label}</span><strong>${value}</strong>`;
    els.accountSummary.appendChild(row);
  });

  renderSettingsContext();
};

const renderSettings = (settings) => {
  els.backendLocalPath.value = settings.backendLocalPath || "";
  els.backendBaseUrl.value = settings.backendBaseUrl || "";
  els.preferLocalBackend.checked = canUseLocalBackend() && Boolean(settings.preferLocalBackend);
  els.updaterEndpoint.value = settings.updaterEndpoint || "";
  els.updaterPublicKey.value = settings.updaterPublicKey || "";
  els.minMemory.value = String(settings.minMemoryMb || 2048);
  els.maxMemory.value = String(settings.maxMemoryMb || 4096);

  if (els.backendLocalField) {
    els.backendLocalField.hidden = !canUseLocalBackend();
  }

  if (els.preferLocalBackendRow) {
    els.preferLocalBackendRow.hidden = !canUseLocalBackend();
  }

  els.backendLocalPath.disabled = !canUseLocalBackend();
  els.preferLocalBackend.disabled = !canUseLocalBackend();
};

const formatNewsMeta = (item) => {
  const fragments = [];

  if (item.author) {
    fragments.push(item.author);
  }

  if (item.publishDate) {
    const date = new Date(item.publishDate);
    if (!Number.isNaN(date.getTime())) {
      fragments.push(
        date.toLocaleDateString("es-MX", {
          day: "2-digit",
          month: "short",
          year: "numeric"
        })
      );
    }
  }

  return fragments.join(" • ") || "Backend de Tavari Client";
};

const renderNews = (items) => {
  const safeItems = Array.isArray(items) ? items : [];

  if (state.activeNewsIndex >= safeItems.length) {
    state.activeNewsIndex = 0;
  }

  const activeItem = safeItems[state.activeNewsIndex] || null;
  els.newsCount.textContent = String(safeItems.length);
  els.newsList.innerHTML = "";

  if (!activeItem) {
    els.newsTitle.textContent = "Sin noticias";
    els.newsMeta.textContent = "Backend de Tavari Client";
    els.newsContent.textContent =
      "Aqui apareceran las noticias publicadas en tu backend.";
    return;
  }

  els.newsTitle.textContent = activeItem.title || "Sin titulo";
  els.newsMeta.textContent = formatNewsMeta(activeItem);
  els.newsContent.textContent =
    activeItem.content || "Esta noticia no tiene contenido.";

  safeItems.forEach((item, index) => {
    const button = document.createElement("button");
    const title = document.createElement("strong");
    const meta = document.createElement("span");
    button.type = "button";
    button.className = "news-panel__item";
    button.classList.toggle("is-active", index === state.activeNewsIndex);
    title.textContent = item.title || "Sin titulo";
    meta.textContent = formatNewsMeta(item);
    button.append(title, meta);
    button.addEventListener("click", () => {
      state.activeNewsIndex = index;
      renderNews(newsItems());
    });
    els.newsList.appendChild(button);
  });
};

const applyHero = (instance) => {
  const fallbackTitle = "TAVARI";

  if (!instance) {
    els.selectedServerLabel.textContent = "SERVIDOR TAVARI";
    els.selectedInstanceTitle.textContent = fallbackTitle;
    els.selectedLoaderCopy.textContent = "Selecciona una instancia para jugar.";
    els.sourceChip.textContent = isStaffSession() ? "sin backend" : "Tavari Client";
    els.serverChip.textContent = "sin servidor";
    els.heroImage.style.backgroundImage = "";
    els.playBtn.disabled = true;
    renderSettingsContext();
    return;
  }

  els.selectedServerLabel.textContent =
    (instance.serverLabel || "SERVIDOR PUBLICO").toUpperCase();
  els.selectedInstanceTitle.textContent = instance.name.toUpperCase();

  const fragments = [
    `${instance.loaderType.toUpperCase()} ${instance.loaderVersion}`,
    `Minecraft ${instance.minecraftVersion}`
  ];

  if (instance.maintenance) fragments.push("En mantenimiento");
  if (instance.staffOnly) fragments.push("Solo staff");

  els.selectedLoaderCopy.textContent = fragments.join(" - ");
  els.sourceChip.textContent = isStaffSession()
    ? instance.sourceMode
    : "Tavari Client";
  els.serverChip.textContent = safeServerText(instance);
  els.heroImage.style.backgroundImage = instance.backgroundUrl
    ? `linear-gradient(180deg, rgba(3, 8, 18, 0.26), rgba(3, 8, 18, 0.2)), url("${instance.backgroundUrl}")`
    : "";
  els.playBtn.disabled =
    state.gameLifecycleStatus === "launching" ||
    state.gameLifecycleStatus === "running" ||
    state.gameLifecycleStatus === "closing";
  renderSettingsContext();
};

const renderInstances = (instances) => {
  els.instancesList.innerHTML = "";
  const items = visibleInstances(instances);

  if (!items.length) {
    state.selectedInstanceKey = null;
    applyHero(null);
    return;
  }

  if (!state.selectedInstanceKey || !items.some((item) => item.key === state.selectedInstanceKey)) {
    state.selectedInstanceKey = items[0].key;
  }

  items.forEach((instance) => {
    const node = els.instanceTemplate.content.firstElementChild.cloneNode(true);
    const art = node.querySelector(".rail-instance__art");
    const label = node.querySelector(".rail-instance__name");
    const initials = initialsFromText(instance.name);

    node.title = instance.name;
    label.textContent = instance.name;
    node.classList.toggle("is-selected", state.selectedInstanceKey === instance.key);

    if (instance.iconUrl || instance.thumbnailUrl) {
      art.style.backgroundImage = `url("${instance.iconUrl || instance.thumbnailUrl}")`;
      art.textContent = "";
    } else {
      art.textContent = initials;
      art.style.display = "grid";
      art.style.placeItems = "center";
      art.style.fontFamily = '"Agency FB", "Bahnschrift", sans-serif';
      art.style.fontSize = "1.1rem";
      art.style.fontWeight = "800";
      art.style.letterSpacing = "0.08em";
    }

    node.addEventListener("click", () => {
      state.selectedInstanceKey = instance.key;
      renderInstances(state.bootstrap.instances);
    });

    els.instancesList.appendChild(node);
  });

  applyHero(selectedInstance());
};

const renderBootstrap = (bootstrap) => {
  state.bootstrap = bootstrap;
  state.bootstrapFingerprint = bootstrap?.backendFingerprint || "";
  applyAppMeta(bootstrap);
  renderSettings(bootstrap.settings);
  renderAccount(bootstrap.account);
  renderInstances(bootstrap.instances);
  renderNews(bootstrap.news);
  applyStaffVisibility(bootstrap);
  setActiveSettingsTab(state.activeSettingsTab);
  setView(bootstrap.account ? "dashboard" : "login");
  resetGameLifecycle();
  setStatus("Listo", bootstrap.backendSummary);
};

const refreshBootstrap = async () => {
  setStatus("Cargando", "Cargando Tavari Client.");
  const bootstrap = await invoke("get_bootstrap");
  renderBootstrap(bootstrap);
};

const refreshBootstrapSilently = async () => {
  const bootstrap = await invoke("get_bootstrap");
  const nextFingerprint = bootstrap?.backendFingerprint || "";
  const changed =
    Boolean(state.bootstrap) && Boolean(nextFingerprint) && nextFingerprint !== state.bootstrapFingerprint;

  renderBootstrap(bootstrap);

  if (changed && state.gameLifecycleStatus === "idle") {
    setStatus("Backend actualizado", "Se detectaron cambios nuevos en el backend.");
    pushNotification(
      "Launcher actualizado",
      "Se detectaron cambios nuevos en el backend y el launcher ya refresco los datos."
    );
  }
};

const pollBackendChanges = async ({ force = false } = {}) => {
  if (
    state.backendPollInFlight ||
    state.settingsOpen ||
    state.gameLifecycleStatus !== "idle"
  ) {
    return;
  }

  if (!force && Date.now() - state.lastBackendPollAt < BACKEND_POLL_DEBOUNCE_MS) {
    return;
  }

  state.backendPollInFlight = true;
  state.lastBackendPollAt = Date.now();

  try {
    await refreshBootstrapSilently();
  } catch (_error) {
    // Polling silencioso: no interrumpe al usuario si GitHub Pages tarda en reflejar cambios.
  } finally {
    state.backendPollInFlight = false;
  }
};

const readSettingsForm = () => ({
  ...state.bootstrap.settings,
  backendLocalPath: els.backendLocalPath.value.trim(),
  backendBaseUrl: els.backendBaseUrl.value.trim(),
  preferLocalBackend: canUseLocalBackend() && els.preferLocalBackend.checked,
  updaterEndpoint: els.updaterEndpoint.value.trim(),
  updaterPublicKey: els.updaterPublicKey.value.trim(),
  minMemoryMb: Number(els.minMemory.value || 2048),
  maxMemoryMb: Number(els.maxMemory.value || 4096)
});

els.offlineLoginBtn.addEventListener("click", async () => {
  const username = els.offlineUsername.value.trim();
  if (!username) {
    setStatus("Falta usuario", "Escribe un nombre no premium de 3 a 16 caracteres.");
    return;
  }

  setStatus("Autenticando", "Creando perfil offline.");
  try {
    await invoke("login_offline", { username });
    await refreshBootstrap();
    setView("dashboard");
  } catch (error) {
    setErrorStatus(error, "No fue posible iniciar sesion.");
  }
});

  els.microsoftLoginBtn.addEventListener("click", async () => {
    els.deviceBox.classList.add("hidden");
    setStatus("Microsoft", "Preparando inicio de sesion premium.");

    try {
      await invoke("login_microsoft");
      await refreshBootstrap();
      setView("dashboard");
  } catch (error) {
    setErrorStatus(error, "No fue posible iniciar sesion con Microsoft.");
  }
});

els.logoutBtn.addEventListener("click", performLogout);
els.settingsLogoutBtn.addEventListener("click", async () => {
  if (!state.bootstrap?.account) return;
  await performLogout();
});

els.saveSettingsBtn.addEventListener("click", async () => {
  try {
    const settings = readSettingsForm();
    await invoke("save_settings", { settings });
    await refreshBootstrap();
    setStatus("Ajustes guardados", "La configuracion local fue actualizada.");
  } catch (error) {
    setErrorStatus(error, "No fue posible guardar la configuracion.");
  }
});

els.toggleSettingsBtn.addEventListener("click", () => {
  setSettingsOpen(!state.settingsOpen);
});

els.notificationsBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  setNotificationsOpen(!state.notificationsOpen);
});

els.clearNotificationsBtn.addEventListener("click", () => {
  state.notifications = [];
  state.notificationsUnread = 0;
  renderNotifications();
});

els.closeSettingsBtn.addEventListener("click", () => {
  setSettingsOpen(false);
});

els.settingsBackdrop.addEventListener("click", () => {
  setSettingsOpen(false);
});

els.settingsBackBtn.addEventListener("click", () => {
  setSettingsOpen(false);
});

els.settingsNavButtons.forEach((button) => {
  button.addEventListener("click", () => {
    setActiveSettingsTab(button.dataset.settingsTab);
  });
});

els.playBtn.addEventListener("click", async () => {
  if (!state.selectedInstanceKey) return;

  state.gameLifecycleStatus = "launching";
  state.currentGameInstanceKey = state.selectedInstanceKey;
  setStatus("Preparando", "Preparando cliente.");
  setStatusProgress(0.04, true);
  els.playBtn.disabled = true;
  els.playBtn.textContent = "Abriendo...";
  els.closeGameBtn.classList.add("is-hidden");
  els.closeGameBtn.disabled = true;

  try {
    await invoke("launch_instance", {
      instanceKey: state.selectedInstanceKey
    });
  } catch (error) {
    resetGameLifecycle();
    setErrorStatus(error, "No fue posible iniciar el juego.");
  }
});

els.closeGameBtn.addEventListener("click", async () => {
  els.closeGameBtn.disabled = true;
  try {
    await invoke("close_running_game");
  } catch (error) {
    els.closeGameBtn.disabled = false;
    setErrorStatus(error, "No fue posible cerrar el juego.");
  }
});

await listen("microsoft-device-code", (event) => {
  const payload = event.payload;
  els.deviceBox.classList.remove("hidden");
  els.deviceMessage.textContent = payload.message;
  els.deviceCode.textContent = payload.userCode;
  els.deviceLink.href = payload.verificationUri;
  setStatus(
    "Microsoft",
    "Se abrio la ventana de Microsoft. Si no aparece, usa el enlace y el codigo."
  );
});

await listen("launcher-status", (event) => {
  applyLauncherStage(event.payload);
});

await listen("sync-progress", (event) => {
  applySyncProgress(event.payload);
});

await listen("game-lifecycle", (event) => {
  applyGameLifecycle(event.payload);
});

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    if (state.notificationsOpen) {
      setNotificationsOpen(false);
      return;
    }

    if (state.settingsOpen) {
      setSettingsOpen(false);
    }
  }
});

document.addEventListener("click", (event) => {
  if (
    state.notificationsOpen &&
    !els.notificationsPanel.contains(event.target) &&
    !els.notificationsBtn.contains(event.target)
  ) {
    setNotificationsOpen(false);
  }
});

setActiveSettingsTab(state.activeSettingsTab);
applyAdaptiveScale();
renderNotifications();

window.addEventListener("resize", applyAdaptiveScale);
window.visualViewport?.addEventListener("resize", applyAdaptiveScale);
window.addEventListener("focus", () => {
  void pollBackendChanges({ force: true });
});
window.addEventListener("online", () => {
  void pollBackendChanges({ force: true });
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    void pollBackendChanges({ force: true });
  }
});

refreshBootstrap().catch((error) => {
  setErrorStatus(error, "No fue posible cargar el launcher.");
});

window.setTimeout(() => {
  void pollBackendChanges({ force: true });
}, 2500);

window.setInterval(() => {
  void pollBackendChanges();
}, BACKEND_POLL_INTERVAL_MS);
