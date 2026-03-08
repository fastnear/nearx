import { ChevronLeft, ChevronRight } from "lucide-react";
import { useRouteHistory } from "../hooks/useRouteHistory";

export default function BackForwardControls() {
  const { canGoBack, canGoForward, goBack, goForward } = useRouteHistory();
  const buttonClass =
    "flex size-9 items-center justify-center rounded border border-gray-200 bg-surface text-gray-500 hover:bg-gray-50 hover:text-gray-800 disabled:cursor-not-allowed disabled:opacity-40";

  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        onClick={goBack}
        disabled={!canGoBack}
        className={buttonClass}
        title="Back"
        aria-label="Go back"
      >
        <ChevronLeft className="size-5" />
      </button>
      <button
        type="button"
        onClick={goForward}
        disabled={!canGoForward}
        className={buttonClass}
        title="Forward"
        aria-label="Go forward"
      >
        <ChevronRight className="size-5" />
      </button>
    </div>
  );
}
