import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ChevronDown } from "lucide-react";
import type { ModelInfo } from "@/bindings";
import type { ModelCardStatus } from "./ModelCard";
import ModelCard, { isLegacySource } from "./ModelCard";
import AbraxLogo from "../icons/AbraxLogo";
import { useModelStore } from "../../stores/modelStore";
import {
  confirmarBorrado,
  confirmarDescarga,
} from "../../lib/utils/modelDialogs";

/**
 * A partir de aquí el catálogo se considera «corto»: caben todas las tarjetas
 * sin scroll y plegarlas tras «Ver los N modelos» solo agrega un clic. Con el
 * catálogo curado (5 modelos) siempre se muestran todas; el plegado se conserva
 * para el caso de un usuario con muchos modelos heredados en disco.
 */
const CATALOGO_CORTO = 8;

interface OnboardingProps {
  onModelSelected: () => void;
}

const Onboarding: React.FC<OnboardingProps> = ({ onModelSelected }) => {
  const { t, i18n } = useTranslation();
  const {
    models,
    downloadModel,
    selectModel,
    downloadingModels,
    verifyingModels,
    extractingModels,
    downloadProgress,
    downloadStats,
    downloadErrors,
    cancelDownload,
    deleteModel,
    currentModel,
  } = useModelStore();
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const hasStartedSelection = useRef(false);

  const isBusy = selectedModelId !== null;

  // The UI locale's base code ("es-CL" → "es") is the best signal available at
  // onboarding for what the user will dictate: the dictation language setting
  // doesn't exist yet at this step.
  const uiLanguage = (i18n.resolvedLanguage ?? i18n.language ?? "en").split(
    "-",
  )[0];

  // Curate the download list: legacy (.bin/ONNX) downloads are deprecated and
  // never shown here (they still appear in the compatible section if already on
  // disk). The catalog arrives rank-sorted, but models that can't transcribe
  // the user's language must not lead: without this, a Spanish-speaking new
  // user gets English-only Parakeet as pick #1. Within each group the catalog
  // order is preserved. Everything else hides behind "Show all".
  const { downloadable, topPicks, otherRecommended, rest } = useMemo(() => {
    const speaksUiLanguage = (m: ModelInfo) =>
      m.supported_languages.includes(uiLanguage);
    const uiLanguageFirst = (list: ModelInfo[]) => [
      ...list.filter(speaksUiLanguage),
      ...list.filter((m) => !speaksUiLanguage(m)),
    ];
    const downloadable = models.filter(
      (m: ModelInfo) => !m.is_downloaded && !isLegacySource(m),
    );
    const recommended = uiLanguageFirst(
      downloadable.filter((m: ModelInfo) => m.is_recommended),
    );
    // `models` arrives in editorial rank order (the backend sorts by rank_of,
    // then accuracy), so keep that order here: ranked-but-not-recommended models
    // surface first, then the unranked tail by accuracy.
    const rest = uiLanguageFirst(
      downloadable.filter((m: ModelInfo) => !m.is_recommended),
    );
    return {
      downloadable,
      topPicks: recommended.slice(0, 2),
      otherRecommended: recommended.slice(2),
      rest,
    };
  }, [models, uiLanguage]);

  const hasRecommended = topPicks.length > 0 || otherRecommended.length > 0;
  const listaCorta = downloadable.length <= CATALOGO_CORTO;
  // When nothing recommended remains to download (e.g. all already on disk),
  // there is no curated subset to collapse, so just show the full list.
  const showRest = showAll || !hasRecommended || listaCorta;

  // Watch for the selected model to finish downloading + verifying + extracting
  useEffect(() => {
    if (!selectedModelId) {
      hasStartedSelection.current = false;
      return;
    }

    const model = models.find((m) => m.id === selectedModelId);
    const stillDownloading = selectedModelId in downloadingModels;
    const stillVerifying = selectedModelId in verifyingModels;
    const stillExtracting = selectedModelId in extractingModels;

    if (
      model?.is_downloaded &&
      !stillDownloading &&
      !stillVerifying &&
      !stillExtracting &&
      !hasStartedSelection.current
    ) {
      hasStartedSelection.current = true;

      // Model is ready — select it and transition
      selectModel(selectedModelId).then((success) => {
        if (success) {
          onModelSelected();
        } else {
          toast.error(t("onboarding.errors.selectModel"));
          hasStartedSelection.current = false;
          setSelectedModelId(null);
        }
      });
    }
  }, [
    selectedModelId,
    models,
    downloadingModels,
    verifyingModels,
    extractingModels,
    selectModel,
    onModelSelected,
    t,
  ]);

  const handleDownloadModel = async (modelId: string) => {
    // Preguntar ANTES de bajar: son cientos de MB y hasta ahora bastaba un clic
    // en la tarjeta. El diálogo es también el «ver qué es antes de bajarlo».
    const model = models.find((m: ModelInfo) => m.id === modelId);
    if (model && !(await confirmarDescarga(model, t))) return;

    setSelectedModelId(modelId);

    // Error toast is handled centrally by the model-download-failed event listener
    // in modelStore — no toast here to avoid duplicates.
    const success = await downloadModel(modelId);
    if (!success) {
      setSelectedModelId(null);
    }
  };

  const handleDeleteModel = async (modelId: string) => {
    const model = models.find((m: ModelInfo) => m.id === modelId);
    if (!model) return;
    if (!(await confirmarBorrado(model, modelId === currentModel, t))) return;
    try {
      await deleteModel(modelId);
    } catch (err) {
      console.error(`Failed to delete model ${modelId}:`, err);
    }
  };

  const handleCancelDownload = async (modelId: string) => {
    const success = await cancelDownload(modelId);
    if (success) {
      setSelectedModelId(null);
    }
  };

  const handleSelectExistingModel = (modelId: string) => {
    setSelectedModelId(modelId);
  };

  const getModelStatus = (modelId: string): ModelCardStatus => {
    if (modelId in extractingModels) return "extracting";
    if (modelId in verifyingModels) return "verifying";
    if (modelId in downloadingModels) return "downloading";
    return "downloadable";
  };

  const getExistingModelStatus = (modelId: string): ModelCardStatus => {
    if (selectedModelId === modelId) return "switching";
    return "available";
  };

  const getModelDownloadProgress = (modelId: string): number | undefined => {
    return downloadProgress[modelId]?.percentage;
  };

  const getModelDownloadSpeed = (modelId: string): number | undefined => {
    return downloadStats[modelId]?.speed;
  };

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-4 inset-0">
      <div className="flex flex-col items-center gap-2 shrink-0">
        <AbraxLogo width={240} withTagline />
        <p className="text-text/70 max-w-md font-medium mx-auto">
          {t("onboarding.subtitle")}
        </p>
      </div>

      <div className="max-w-[600px] w-full mx-auto text-center flex-1 flex flex-col min-h-0">
        <div className="space-y-6 pb-6">
          {/* F4: el catálogo también tiene estados — cargando y vacío/fallo,
              en vez de una pantalla en blanco. */}
          {models.length === 0 && (
            <p className="text-sm text-text/60 py-8">
              {t("onboarding.emptyCatalog")}
            </p>
          )}
          {models.some((m: ModelInfo) => m.is_downloaded) && (
            <div className="space-y-3">
              <div className="text-left">
                <h2 className="text-sm font-medium text-text/60">
                  {t("onboarding.existingModelsTitle")}
                </h2>
              </div>
              {models
                .filter((m: ModelInfo) => m.is_downloaded)
                .map((model: ModelInfo) => (
                  <ModelCard
                    key={model.id}
                    model={model}
                    status={getExistingModelStatus(model.id)}
                    disabled={isBusy}
                    onSelect={handleSelectExistingModel}
                    onDelete={handleDeleteModel}
                    showRecommended={false}
                  />
                ))}
            </div>
          )}

          {downloadable.length > 0 && (
            <div className="space-y-3">
              <div className="text-left">
                <h2 className="text-sm font-medium text-text/60">
                  {t("onboarding.downloadModelsTitle")}
                </h2>
              </div>

              {topPicks.map((model: ModelInfo) => (
                <ModelCard
                  key={model.id}
                  model={model}
                  variant="featured"
                  status={getModelStatus(model.id)}
                  disabled={isBusy}
                  onSelect={handleDownloadModel}
                  onDownload={handleDownloadModel}
                  onCancel={handleCancelDownload}
                  downloadProgress={getModelDownloadProgress(model.id)}
                  downloadSpeed={getModelDownloadSpeed(model.id)}
                  errorMessage={downloadErrors[model.id]}
                  showRecommended
                />
              ))}

              {otherRecommended.map((model: ModelInfo) => (
                <ModelCard
                  key={model.id}
                  model={model}
                  status={getModelStatus(model.id)}
                  disabled={isBusy}
                  onSelect={handleDownloadModel}
                  onDownload={handleDownloadModel}
                  onCancel={handleCancelDownload}
                  downloadProgress={getModelDownloadProgress(model.id)}
                  downloadSpeed={getModelDownloadSpeed(model.id)}
                  errorMessage={downloadErrors[model.id]}
                  showRecommended
                />
              ))}

              {hasRecommended && rest.length > 0 && !listaCorta && (
                <button
                  type="button"
                  onClick={() => setShowAll((v) => !v)}
                  className="flex items-center justify-center gap-1.5 mx-auto py-1 text-sm font-medium text-text/60 hover:text-text transition-colors"
                >
                  {showAll
                    ? t("onboarding.showFewerModels")
                    : t("onboarding.showAllModels", {
                        total: downloadable.length,
                      })}
                  <ChevronDown
                    className={`w-4 h-4 transition-transform duration-200 ${
                      showAll ? "rotate-180" : ""
                    }`}
                  />
                </button>
              )}

              {showRest &&
                rest.map((model: ModelInfo) => (
                  <ModelCard
                    key={model.id}
                    model={model}
                    status={getModelStatus(model.id)}
                    disabled={isBusy}
                    onSelect={handleDownloadModel}
                    onDownload={handleDownloadModel}
                    onCancel={handleCancelDownload}
                    downloadProgress={getModelDownloadProgress(model.id)}
                    downloadSpeed={getModelDownloadSpeed(model.id)}
                    errorMessage={downloadErrors[model.id]}
                    showRecommended
                  />
                ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default Onboarding;
