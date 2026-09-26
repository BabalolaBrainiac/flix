import { describe, it, expect } from "vitest";
import { computeUpdateDialogState } from "./update";
import type { UpdateStatus } from "./types";

describe("Update dialog state transitions", () => {
  const currentStatus: UpdateStatus = {
    current_version: "0.4.0",
    latest_version: "0.4.0",
    available: false,
    release_url: "https://github.com/BabalolaBrainiac/flix/releases/tag/v0.4.0",
  };

  const availableStatus: UpdateStatus = {
    current_version: "0.4.0",
    latest_version: "0.4.1",
    available: true,
    release_url: "https://github.com/BabalolaBrainiac/flix/releases/tag/v0.4.1",
  };

  it("handles the initial loading state", () => {
    const state = computeUpdateDialogState({
      busy: true,
      status: null,
      error: "",
      message: "",
    });

    expect(state.kind).toBe("loading");
    expect(state.title).toBe("Checking for updates");
    expect(state.description).toContain("checking");
    expect(state.isInstallDisabled).toBe(true);
  });

  it("handles the error state and offers retry", () => {
    const state = computeUpdateDialogState({
      busy: false,
      status: null,
      error: "Network connection lost",
      message: "",
    });

    expect(state.kind).toBe("error");
    expect(state.errorMessage).toBe("Network connection lost");
    expect(state.actionLabel).toBe("Try Again");
    expect(state.canRetry).toBe(true);
    expect(state.isInstallDisabled).toBe(true);
  });

  it("handles the current version state", () => {
    const state = computeUpdateDialogState({
      busy: false,
      status: currentStatus,
      error: "",
      message: "",
    });

    expect(state.kind).toBe("current");
    expect(state.description).toBe("Flix 0.4.0 is the latest stable version.");
    expect(state.isInstallDisabled).toBe(true);
  });

  it("handles available update state ready for installation", () => {
    const state = computeUpdateDialogState({
      busy: false,
      status: availableStatus,
      error: "",
      message: "",
    });

    expect(state.kind).toBe("available");
    expect(state.description).toBe("Flix 0.4.1 is available. You have 0.4.0.");
    expect(state.actionLabel).toBe("Download and Install");
    expect(state.isInstallDisabled).toBe(false);
  });

  it("disables the install button during active update preparation", () => {
    const state = computeUpdateDialogState({
      busy: true,
      status: availableStatus,
      error: "",
      message: "",
    });

    expect(state.kind).toBe("available");
    expect(state.actionLabel).toBe("Preparing Update…");
    expect(state.isInstallDisabled).toBe(true);
  });

  it("handles success state and disables button after launch", () => {
    const state = computeUpdateDialogState({
      busy: false,
      status: availableStatus,
      error: "",
      message: "The verified update is open. Replace Flix in Applications to finish.",
    });

    expect(state.kind).toBe("available");
    expect(state.successMessage).toBe(
      "The verified update is open. Replace Flix in Applications to finish."
    );
    expect(state.actionLabel).toBe("Update Started");
    expect(state.isInstallDisabled).toBe(true);
  });
});
