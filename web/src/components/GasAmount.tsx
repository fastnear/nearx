import { useState } from "react";

export default function GasAmount({ gas }: { gas: number }) {
  const [showRaw, setShowRaw] = useState(false);

  return (
    <span
      className="cursor-pointer hover:underline decoration-dotted"
      onClick={() => setShowRaw(!showRaw)}
      title={showRaw ? "Click to show in Tgas" : "Click to show in Gas"}
    >
      {showRaw ? (
        <>{gas.toLocaleString()} Gas</>
      ) : (
        <>{(gas / 1e12).toFixed(2)} Tgas</>
      )}
    </span>
  );
}
