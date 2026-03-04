(function () {
  const inject = () => {
    if (document.getElementById("nearx-open-btn")) return;
    const m = location.href.match(/tx\/([A-Za-z0-9._-]{6,})/);
    if (!m) return;
    const btn = document.createElement("button");
    btn.id = "nearx-open-btn";
    btn.textContent = "Open in NEARx";
    btn.style = "position:fixed;bottom:16px;right:16px;padding:8px 12px;z-index:999999;background:#667eea;color:white;border:none;border-radius:4px;cursor:pointer;";
    btn.onclick = () => {
      chrome.runtime.sendMessage({ type: "open_deeplink", url: `nearx://v1/tx/${m[1]}` });
    };
    document.body.appendChild(btn);
  };
  new MutationObserver(inject).observe(document.documentElement, { childList: true, subtree: true });
  inject();
})();
