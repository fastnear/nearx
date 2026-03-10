import { useCallback, useState } from "react";
import type { ReactNode } from "react";
import { Check, Copy } from "lucide-react";

interface CopyableValueProps {
  text: string;
  children: ReactNode;
}

export default function CopyableValue({ text, children }: CopyableValueProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  return (
    <span className="inline-flex items-baseline gap-1.5">
      {children}
      <button
        type="button"
        onClick={handleCopy}
        className="inline-flex shrink-0 translate-y-[1px] cursor-pointer items-center text-gray-400 hover:text-gray-600"
        title="Copy to clipboard"
      >
        {copied ? <Check size={12} className="text-green-600" /> : <Copy size={12} />}
      </button>
    </span>
  );
}
