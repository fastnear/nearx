import { Link } from "react-router-dom";
import { CircleAlert } from "lucide-react";
import useSpamTokens, { isSpam } from "../hooks/useSpamTokens";
import useSpamNfts, { isSpamNft } from "../hooks/useSpamNfts";

const DEFAULT_MAX_LEN = 30;

function SpamBadge() {
  return (
    <span title="Flagged as spam">
      <CircleAlert className="inline-block size-3 shrink-0 text-red-500" />
    </span>
  );
}

/**
 * @param maxLength - number of chars before truncating, or "auto" for
 *   CSS-only truncation that fills available space. Defaults to 30.
 */
export default function AccountId({
  accountId,
  linked = true,
  maxLength = DEFAULT_MAX_LEN,
}: {
  accountId: string;
  linked?: boolean;
  maxLength?: number | "auto";
}) {
  const spamSet = useSpamTokens();
  const spamNfts = useSpamNfts();
  const spam = isSpam(spamSet, accountId) || isSpamNft(spamNfts, accountId);

  const auto = maxLength === "auto";
  const needsTruncate = auto || accountId.length > maxLength;

  const truncateCls = auto
    ? "font-mono text-xs inline-block min-w-0 max-w-full overflow-hidden text-ellipsis whitespace-nowrap align-bottom"
    : "font-mono text-xs inline-block overflow-hidden text-ellipsis whitespace-nowrap align-bottom";
  const normalCls = "font-mono text-xs whitespace-nowrap";
  const innerCls = needsTruncate ? truncateCls : normalCls;
  const maxWidthStyle = needsTruncate && !auto ? { maxWidth: `${maxLength + 6}ch` } : undefined;

  if (linked) {
    return (
      <span className={`inline-flex items-center gap-0.5 ${auto ? "min-w-0 max-w-full" : ""}`}>
        <Link
          to={`/account/${accountId}`}
          className={`${innerCls} text-blue-600 hover:underline`}
          style={maxWidthStyle}
          title={needsTruncate ? accountId : undefined}
        >
          {accountId}
        </Link>
        {spam && <SpamBadge />}
      </span>
    );
  }

  return (
    <span className={`inline-flex items-center gap-0.5 ${auto ? "min-w-0 max-w-full" : ""}`}>
      <span
        className={innerCls}
        style={maxWidthStyle}
        title={needsTruncate ? accountId : undefined}
      >
        {accountId}
      </span>
      {spam && <SpamBadge />}
    </span>
  );
}
