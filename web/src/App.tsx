import { useEffect } from "react";
import {
  BrowserRouter,
  HashRouter,
  Navigate,
  Route,
  Routes,
  useNavigate,
} from "react-router-dom";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import BlockDetail from "./pages/BlockDetail";
import TxDetail from "./pages/TxDetail";
import AccountDetail from "./pages/AccountDetail";
import SignTransaction from "./pages/SignTransaction";
import Staking from "./pages/Staking";
import { mapCanonicalDeepLinkToRoute } from "./tauri/deeplink";
import {
  getRuntimeConfig,
  isTauriRuntime,
  openExternal,
  subscribeDeepLinks,
} from "./tauri/runtime";

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
      <ExplorerRoutes />
    </Router>
  );
}
