import { useEffect, useRef } from "react";
import {
  BrowserRouter,
  HashRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import BlockDetail from "./pages/BlockDetail";
import TxDetail from "./pages/TxDetail";
import AccountDetail from "./pages/AccountDetail";
import SignTransaction from "./pages/SignTransaction";
import Staking from "./pages/Staking";
import Settings from "./pages/Settings";
import { mapCanonicalDeepLinkToRoute } from "./tauri/deeplink";
import {
  getRuntimeConfig,
  isTauriRuntime,
  openExternal,
  subscribeDeepLinks,
} from "./tauri/runtime";
import { getPersistedLastRoute } from "./hooks/useRouteHistory";

function RuntimeBridge() {
  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let cancelled = false;

    void getRuntimeConfig().then((cfg) => {
      if (!cfg || cancelled) {
        return;
      }

      document.documentElement.dataset.nearxBrokerAvailable = cfg.broker_available
        ? "true"
        : "false";
    });

    return () => {
      cancelled = true;
    };
  }, []);

  return null;
}

function DeepLinkBridge() {
  const navigate = useNavigate();

  useEffect(() => {
    let disposed = false;
    let unlisten = () => {};

    const forward = (url: string) => {
      const route = mapCanonicalDeepLinkToRoute(url) ?? "/";
      navigate(route);
    };

    void subscribeDeepLinks((url) => {
      if (!disposed) {
        forward(url);
      }
    }).then((stop) => {
      if (disposed) {
        stop();
        return;
      }

      unlisten = stop;
    });

    return () => {
      disposed = true;
      unlisten();
    };
  }, [navigate]);

  return null;
}

function LastRouteRestore() {
  const navigate = useNavigate();
  const location = useLocation();
  const restored = useRef(false);

  useEffect(() => {
    if (restored.current) return;
    restored.current = true;

    // Only restore when landing on the root route
    if (location.pathname !== "/") return;

    const saved = getPersistedLastRoute();
    if (saved) {
      navigate(saved, { replace: true });
    }
  }, []);

  return null;
}

function ExternalLinkBridge() {
  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    const onClick = (event: MouseEvent) => {
      if (!(event.target instanceof Element)) {
        return;
      }

      const anchor = event.target.closest("a[href]") as HTMLAnchorElement | null;
      if (!anchor || anchor.target !== "_blank") {
        return;
      }

      const href = anchor.href;
      if (!href) {
        return;
      }

      try {
        const url = new URL(href);
        if (
          (url.protocol === "http:" || url.protocol === "https:") &&
          url.origin === window.location.origin
        ) {
          return;
        }
      } catch {
        return;
      }

      event.preventDefault();
      void openExternal(href);
    };

    document.addEventListener("click", onClick);

    return () => {
      document.removeEventListener("click", onClick);
    };
  }, []);

  return null;
}

function ExplorerRoutes() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Home />} />
        <Route path="block/:blockId" element={<BlockDetail />} />
        <Route path="tx/:txHash" element={<TxDetail />} />
        <Route path="account/:accountId" element={<AccountDetail />} />
        {isTauriRuntime() && (
          <Route path="sign" element={<SignTransaction />} />
        )}
        {isTauriRuntime() && (
          <Route path="staking" element={<Staking />} />
        )}
        {isTauriRuntime() && (
          <Route path="settings" element={<Settings />} />
        )}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}

export default function App() {
  const Router = isTauriRuntime() ? HashRouter : BrowserRouter;

  return (
    <Router>
      <RuntimeBridge />
      <DeepLinkBridge />
      <ExternalLinkBridge />
      <LastRouteRestore />
      <ExplorerRoutes />
    </Router>
  );
}
