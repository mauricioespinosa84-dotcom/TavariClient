const tauri = window.__TAURI__;

if (!tauri?.core?.invoke) {
  throw new Error("Tauri API unavailable in splashscreen");
}

const { invoke } = tauri.core;
const { listen } = tauri.event;

const els = {
  stage: document.querySelector("#startup-stage"),
  detail: document.querySelector("#startup-detail"),
  progress: document.querySelector("#startup-progress"),
  percent: document.querySelector("#startup-percent"),
  version: document.querySelector("#startup-version")
};

const renderProgress = (payload) => {
  els.stage.textContent = payload.stage || "Iniciando";
  els.detail.textContent = payload.detail || "Preparando Tavari Client.";

  const versionText = payload.version
    ? `Version ${payload.version}`
    : payload.status === "ready"
      ? "Launch ready"
      : "Loading";
  els.version.textContent = versionText;

  els.progress.classList.toggle("is-indeterminate", Boolean(payload.indeterminate));

  if (payload.indeterminate || typeof payload.progress !== "number") {
    els.progress.style.width = "";
    els.percent.textContent = "...";
    return;
  }

  const clamped = Math.max(0, Math.min(1, payload.progress));
  els.progress.style.width = `${Math.round(clamped * 100)}%`;
  els.percent.textContent = `${Math.round(clamped * 100)}%`;
};

await listen("startup-progress", (event) => {
  renderProgress(event.payload);
});

invoke("startup_ready").catch((error) => {
  renderProgress({
    status: "error",
    stage: "Error de inicio",
    detail: String(error || "No fue posible iniciar Tavari Client."),
    progress: 1,
    version: null,
    indeterminate: false
  });
});
