import type { UpdateStatus } from "./types";

export type UpdateDialogStateKind = "loading" | "error" | "available" | "current";

export interface UpdateDialogViewModel {
  title: string;
  description: string;
  kind: UpdateDialogStateKind;
  errorMessage?: string;
  successMessage?: string;
  actionLabel?: string;
  isInstallDisabled: boolean;
  canRetry: boolean;
}

export function computeUpdateDialogState(params: {
  busy: boolean;
  status: UpdateStatus | null;
  error: string;
  message: string;
}): UpdateDialogViewModel {
  if (params.busy && !params.status) {
    return {
      title: "Checking for updates",
      description: "Flix is checking the latest stable release.",
      kind: "loading",
      isInstallDisabled: true,
      canRetry: false,
    };
  }

  if (params.error) {
    return {
      title: "Software Update",
      description: params.error,
      kind: "error",
      errorMessage: params.error,
      actionLabel: "Try Again",
      isInstallDisabled: true,
      canRetry: true,
    };
  }

  if (params.status?.available) {
    const actionLabel = params.busy
      ? "Preparing Update…"
      : params.message
        ? "Update Started"
        : "Download and Install";

    return {
      title: "Software Update",
      description: `Flix ${params.status.latest_version} is available. You have ${params.status.current_version}.`,
      kind: "available",
      successMessage: params.message || undefined,
      actionLabel,
      isInstallDisabled: params.busy || Boolean(params.message),
      canRetry: false,
    };
  }

  if (params.status) {
    return {
      title: "Software Update",
      description: `Flix ${params.status.current_version} is the latest stable version.`,
      kind: "current",
      isInstallDisabled: true,
      canRetry: false,
    };
  }

  return {
    title: "Software Update",
    description: "",
    kind: "current",
    isInstallDisabled: true,
    canRetry: false,
  };
}
