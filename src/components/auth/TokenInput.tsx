import { Check, Copy, Eye, EyeOff } from "lucide-react";
import { useId, useState } from "react";
import { useI18n } from "../../lib/i18n";
import { copySensitiveText } from "../../lib/routeCredentialTransfer";

type TokenInputProps = {
  autoComplete?: string;
  className?: string;
  copy?: boolean;
  label: string;
  onChange: (value: string) => void;
  placeholder?: string;
  value: string;
};

export function TokenInput({
  autoComplete = "current-password",
  className = "",
  copy = false,
  label,
  onChange,
  placeholder,
  value,
}: TokenInputProps) {
  const { t } = useI18n();
  const inputId = useId();
  const [visible, setVisible] = useState(false);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copySensitiveText(value);
    setCopied(true);
  };

  return (
    <div className={`flex flex-col gap-1.5 ${className}`}>
      <label className="text-[12px] font-semibold text-stone-600" htmlFor={inputId}>
        {label}
      </label>
      <div className="relative">
        <input
          aria-label={label}
          autoComplete={autoComplete}
          className={`w-full rounded-xl border border-stone-200 bg-white py-2 pl-3 ${
            copy ? "pr-20" : "pr-11"
          } text-[13px] font-medium text-stone-900 shadow-sm outline-none motion-control focus:border-blue-400 focus:ring-2 focus:ring-blue-100`}
          id={inputId}
          onChange={(event) => onChange(event.target.value)}
          placeholder={placeholder}
          type={visible ? "text" : "password"}
          value={value}
        />
        <div className="absolute inset-y-0 right-0 flex items-center gap-1 pr-2">
          <button
            aria-label={visible ? t("auth.hideToken") : t("auth.showToken")}
            className="grid h-7 w-7 place-items-center rounded-lg text-stone-400 transition-colors hover:bg-stone-100 hover:text-stone-600 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
            onClick={() => setVisible((current) => !current)}
            type="button"
          >
            {visible ? (
              <EyeOff aria-hidden="true" className="h-4 w-4" />
            ) : (
              <Eye aria-hidden="true" className="h-4 w-4" />
            )}
          </button>
          {copy ? (
            <button
              aria-label={copied ? t("auth.tokenCopied") : t("auth.copyToken")}
              className="grid h-7 w-7 place-items-center rounded-lg text-stone-400 transition-colors hover:bg-stone-100 hover:text-stone-600 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={!value.trim()}
              onClick={() => void handleCopy()}
              type="button"
            >
              {copied ? (
                <Check aria-hidden="true" className="h-4 w-4" />
              ) : (
                <Copy aria-hidden="true" className="h-4 w-4" />
              )}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
