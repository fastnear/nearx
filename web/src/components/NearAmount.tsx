import { useState } from "react";
import useTokenPrices, { getFtPrice, computeUsdValue, formatUsd } from "../hooks/useTokenPrices";

const YOCTO = 24;

function isSmallValue(yoctoNear: string): boolean {
  // Less than 0.00001 NEAR (i.e. fewer than YOCTO-5 = 19 digits)
  return yoctoNear.replace(/^0+/, "").length < YOCTO - 4;
}

function formatNearValue(yoctoNear: string): string {
  if (yoctoNear === "0") return "0";
  const s = yoctoNear.padStart(YOCTO + 1, "0");
  const whole = s.slice(0, s.length - YOCTO).replace(/^0+/, "") || "0";
  const frac = s.slice(s.length - YOCTO).replace(/0+$/, "");
  if (!frac) return whole;
  return `${whole}.${frac.slice(0, 5)}`;
}

export default function NearAmount({ yoctoNear, showPrice = false }: { yoctoNear?: string; showPrice?: boolean }) {
  const value = yoctoNear || "0";
  const defaultYocto = value !== "0" && isSmallValue(value);
  const [showYocto, setShowYocto] = useState(defaultYocto);
  const { prices } = useTokenPrices();
  const priceInfo = showPrice ? getFtPrice(prices, "wrap.near") : undefined;
  const usdValue = priceInfo
    ? computeUsdValue(value, priceInfo.decimals, priceInfo.price)
    : 0;

  return (
    <span
      className="cursor-pointer hover:underline decoration-dotted"
      onClick={() => setShowYocto(!showYocto)}
      title={showYocto ? "Click to show in NEAR" : "Click to show in yoctoNEAR"}
    >
      {showYocto ? (
        <>{value} yoctoⓃ</>
      ) : (
        <>{formatNearValue(value)} Ⓝ</>
      )}
      {showPrice && !showYocto && usdValue > 0 && (
        <span className="text-gray-400 font-sans text-xs ml-0.5">{formatUsd(usdValue)}</span>
      )}
    </span>
  );
}
