import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderOpen,
  GitBranch,
  FileCode2,
  FileText,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { commands } from "@/bindings";
import type { DictionaryStats, TermSource } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface DictionarySettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const sourceIcon = (source: TermSource) => {
  switch (source) {
    case "branch":
      return <GitBranch className="w-3 h-3" aria-hidden="true" />;
    case "path":
      return <FileText className="w-3 h-3" aria-hidden="true" />;
    default:
      return <FileCode2 className="w-3 h-3" aria-hidden="true" />;
  }
};

/**
 * Diccionario Vivo (F5): ABRAX aprende la jerga de tu código indexando el
 * proyecto activo. El índice vive en tu disco; nada sale del equipo.
 */
export const DictionarySettings: React.FC<DictionarySettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t, i18n } = useTranslation();
    const { getSetting, refreshSettings } = useSettings();
    const [stats, setStats] = useState<DictionaryStats | null>(null);
    const [indexing, setIndexing] = useState(false);
    const [toggling, setToggling] = useState(false);

    const project = getSetting("dictionary_project") ?? null;

    const loadStats = async () => {
      try {
        const result = await commands.getDictionaryStats();
        setStats(result);
      } catch (e) {
        console.error("Failed to load dictionary stats:", e);
      }
    };

    useEffect(() => {
      loadStats();
    }, [project?.path, project?.last_indexed_ms]);

    const runIndex = async (path: string) => {
      setIndexing(true);
      try {
        const result = await commands.indexProject(path);
        if (result.status === "ok") {
          setStats(result.data);
          toast.success(
            t("settings.advanced.dictionary.indexed", {
              count: result.data.term_count,
            }),
          );
        } else {
          toast.error(
            t("settings.advanced.dictionary.indexError", {
              error: result.error,
            }),
          );
        }
      } finally {
        setIndexing(false);
        await refreshSettings();
      }
    };

    const handlePickProject = async () => {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("settings.advanced.dictionary.pickTitle"),
      });
      if (typeof selected === "string" && selected.length > 0) {
        await runIndex(selected);
      }
    };

    const handleReindex = async () => {
      if (project?.path) await runIndex(project.path);
    };

    const handleToggle = async (enabled: boolean) => {
      setToggling(true);
      try {
        const result = await commands.setDictionaryEnabled(enabled);
        if (result.status !== "ok") {
          toast.error(result.error);
        }
      } finally {
        setToggling(false);
        await refreshSettings();
      }
    };

    const lastIndexed = project?.last_indexed_ms
      ? new Date(project.last_indexed_ms).toLocaleString(i18n.language, {
          dateStyle: "short",
          timeStyle: "short",
        })
      : null;

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.dictionary.title")}
          description={t("settings.advanced.dictionary.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            {project?.path && (
              <Button
                onClick={handleReindex}
                disabled={indexing}
                variant="secondary"
                size="md"
                className="inline-flex items-center gap-1.5"
              >
                {indexing ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="w-3.5 h-3.5" />
                )}
                <span>{t("settings.advanced.dictionary.reindex")}</span>
              </Button>
            )}
            <Button
              onClick={handlePickProject}
              disabled={indexing}
              variant="primary"
              size="md"
              className="inline-flex items-center gap-1.5"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              <span>
                {project
                  ? t("settings.advanced.dictionary.changeProject")
                  : t("settings.advanced.dictionary.pickProject")}
              </span>
            </Button>
          </div>
        </SettingContainer>

        {project && (
          <ToggleSwitch
            checked={project.enabled}
            onChange={handleToggle}
            disabled={toggling || indexing}
            label={t("settings.advanced.dictionary.enable")}
            description={t("settings.advanced.dictionary.enableDescription")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}

        {project && (
          <div
            className={`px-4 py-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} space-y-2`}
          >
            <p className="text-xs text-text/60 break-all" title={project.path}>
              {project.path}
            </p>
            <p className="text-xs text-text/60">
              {stats
                ? t("settings.advanced.dictionary.termCount", {
                    count: stats.term_count,
                  })
                : t("settings.advanced.dictionary.noIndex")}
              {lastIndexed && (
                <span className="text-text/40">
                  {" · "}
                  {t("settings.advanced.dictionary.lastIndexed", {
                    date: lastIndexed,
                  })}
                </span>
              )}
            </p>
            {stats && stats.sample.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {stats.sample.slice(0, 18).map((term) => (
                  <span
                    key={term.term}
                    className="inline-flex items-center gap-1 rounded-md bg-mid-gray/10 border border-mid-gray/20 px-1.5 py-0.5 text-xs text-text/80"
                    title={t(
                      `settings.advanced.dictionary.source.${term.source}`,
                    )}
                  >
                    {sourceIcon(term.source)}
                    <span className="max-w-40 truncate">{term.term}</span>
                  </span>
                ))}
              </div>
            )}
            <p className="text-xs text-text/40">
              {t("settings.advanced.dictionary.privacy")}
            </p>
          </div>
        )}
      </>
    );
  },
);
