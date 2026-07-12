import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { AlertTriangle, Check, Copy } from "lucide-react";
import { commands } from "@/bindings";
import type { CompiledPrompt } from "@/bindings";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";

interface PromptCompilerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Transcripción cruda del dictado a compilar. */
  rawText: string;
}

/**
 * Compilador de Prompts, modo plantilla (F6 MVP): dictado crudo → prompt
 * estructurado (CONTEXTO/TAREA/REQUISITOS/FORMATO) con el linter de
 * ambigüedades es-419. Sin ningún LLM: heurístico, local e instantáneo.
 */
export const PromptCompilerDialog: React.FC<PromptCompilerDialogProps> = ({
  open,
  onOpenChange,
  rawText,
}) => {
  const { t } = useTranslation();
  const [result, setResult] = useState<CompiledPrompt | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    setCopied(false);
    setResult(null);
    commands
      .compilePrompt(rawText)
      .then(setResult)
      .catch((e) => {
        console.error("Failed to compile prompt:", e);
        toast.error(t("settings.history.compiler.error"));
      });
  }, [open, rawText, t]);

  const handleCopy = async () => {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result.markdown);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error("Failed to copy compiled prompt:", e);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("settings.history.compiler.title")}
      description={t("settings.history.compiler.description")}
      closeLabel={t("common.close")}
      className="max-w-2xl"
      footer={
        <Button
          variant="primary"
          size="md"
          onClick={handleCopy}
          disabled={!result}
          className="inline-flex items-center gap-1.5"
        >
          {copied ? (
            <Check className="w-4 h-4" />
          ) : (
            <Copy className="w-4 h-4" />
          )}
          <span>{t("settings.history.compiler.copy")}</span>
        </Button>
      }
    >
      <div className="space-y-3">
        <div>
          <p className="text-xs font-medium text-text/50 uppercase mb-1">
            {t("settings.history.compiler.raw")}
          </p>
          <p className="text-sm italic text-text/70 whitespace-pre-wrap break-words border border-mid-gray/20 rounded-lg px-3 py-2">
            {rawText}
          </p>
        </div>

        {result && result.warnings.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {result.warnings.map((w) => (
              <span
                key={w.term}
                className="inline-flex items-center gap-1 rounded-md border border-amber-500/40 bg-amber-500/10 px-1.5 py-0.5 text-xs text-text/90"
                title={w.question}
              >
                <AlertTriangle className="w-3 h-3 text-amber-500" />
                <span>{w.term}</span>
              </span>
            ))}
          </div>
        )}

        <div>
          <p className="text-xs font-medium text-text/50 uppercase mb-1">
            {t("settings.history.compiler.compiled")}
          </p>
          <pre className="text-xs text-text/90 whitespace-pre-wrap break-words border border-logo-primary/30 bg-logo-primary/5 rounded-lg px-3 py-2 max-h-72 overflow-y-auto font-mono">
            {result ? result.markdown : t("common.loading")}
          </pre>
        </div>
      </div>
    </Dialog>
  );
};
