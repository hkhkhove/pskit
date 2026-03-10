let _pdbeMolstarLoadPromise = null;

//确保PDBe Mol*插件的CSS和JS资源已加载，如果已经加载则直接返回，否则动态添加到页面并等待加载完成。
export async function ensurePdbeMolstarLoaded({
  cssHref = "https://cdn.jsdelivr.net/npm/pdbe-molstar@3.3.0/build/pdbe-molstar.css",
  scriptSrc = "https://cdn.jsdelivr.net/npm/pdbe-molstar@3.3.0/build/pdbe-molstar-plugin.js",
} = {}) {
  if (typeof window === "undefined") return;
  if (window.PDBeMolstarPlugin) return;
  if (_pdbeMolstarLoadPromise) return await _pdbeMolstarLoadPromise;

  _pdbeMolstarLoadPromise = (async () => {
    // CSS
    if (
      cssHref &&
      !document.querySelector(`link[rel="stylesheet"][href="${cssHref}"]`)
    ) {
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = cssHref;
      document.head.appendChild(link);
    }

    // JS
    if (!window.PDBeMolstarPlugin) {
      await new Promise((resolve, reject) => {
        const existing = document.querySelector(`script[src="${scriptSrc}"]`);
        if (existing) {
          existing.addEventListener("load", resolve, { once: true });
          existing.addEventListener("error", reject, { once: true });
          if (window.PDBeMolstarPlugin) resolve();
          return;
        }

        const script = document.createElement("script");
        script.src = scriptSrc;
        script.onload = resolve;
        script.onerror = reject;
        document.body.appendChild(script);
      });
    }
  })();

  return await _pdbeMolstarLoadPromise;
}
// 创建并返回一个新的PDBe Mol*插件实例，确保在调用前已加载插件资源。
export function createPdbeMolstarViewer() {
  if (typeof window === "undefined" || !window.PDBeMolstarPlugin) {
    throw new Error(
      "PDBeMolstarPlugin is not loaded. Call ensurePdbeMolstarLoaded() first.",
    );
  }
  return new window.PDBeMolstarPlugin();
}

export async function destroyPdbeMolstarViewer(viewer) {
  if (!viewer) return;

  try {
    await viewer.clear?.();
  } catch {
    // ignore
  }

  try {
    viewer.plugin?.dispose?.({ doNotForceWebGLContextLoss: true });
  } catch {
    // ignore
  }

  try {
    if (viewer.targetElement?.viewerInstance === viewer) {
      delete viewer.targetElement.viewerInstance;
    }
  } catch {
    // ignore
  }

  try {
    viewer.targetElement = null;
  } catch {
    // ignore
  }
}

function normalizePdbeMolstarFormat(format) {
  const f = String(format || "")
    .toLowerCase()
    .trim();
  // PDBe Mol* expects 'mmcif' (not 'cif').
  if (f === "cif" || f === "bcif") return "mmcif";
  if (f === "mmcif") return "mmcif";
  if (f === "pdb") return "pdb";
  if (f === "sdf") return "sdf";
  return f || "pdb";
}

export async function renderPdbeMolstar(viewer, containerEl, options = {}) {
  if (!viewer) throw new Error("viewer is required");
  if (!containerEl) throw new Error("containerEl is required");

  //确保容器元素具有适当的CSS样式以正确显示Mol*插件（相对定位和隐藏溢出）
  containerEl.style.position = containerEl.style.position || "relative";
  containerEl.style.overflow = containerEl.style.overflow || "hidden";

  const hasRendered = Boolean(viewer?.targetElement);
  if (!hasRendered) {
    return await viewer.render(containerEl, options);
  }
  if (viewer.targetElement !== containerEl) {
    try {
      return await viewer.render(containerEl, options);
    } catch (e) {
      if (e?.message?.includes("already added")) {
        console.warn("Ignoring 'already added' error during PDBe Molstar render", e);
        return;
      }
      throw e;
    }
  }
  if (typeof viewer.clear === "function") {
    await viewer.clear();
  }

  if (options?.customData?.url) {
    const url = options.customData.url;
    const format = normalizePdbeMolstarFormat(options.customData.format);
    const isBinary = Boolean(options.customData.binary);
    const assemblyId = options.assemblyId ?? "";
    const progressMessage = options.customData?.progressMessage;
    return await viewer.load(
      {
        url,
        format,
        isBinary,
        assemblyId,
        progressMessage,
      },
      true,
    );
  }

  if (options?.moleculeId) {
    viewer.initParams = {
      ...(viewer.initParams || {}),
      moleculeId: String(options.moleculeId).toLowerCase(),
      customData: undefined,
    };

    if (typeof viewer.getMoleculeSrcUrl !== "function") {
      throw new Error(
        "PDBeMolstarPlugin.getMoleculeSrcUrl() is not available.",
      );
    }

    const src = viewer.getMoleculeSrcUrl();
    return await viewer.load(
      {
        url: src.url,
        format: normalizePdbeMolstarFormat(src.format),
        isBinary: Boolean(src.isBinary),
        assemblyId: options.assemblyId ?? "",
        progressMessage: options?.progressMessage,
      },
      true,
    );
  }

  return;
}

export function getLoadedStructureCount(viewer) {
  return (
    viewer?.plugin?.managers?.structure?.hierarchy?.current?.structures
      ?.length ?? 0
  );
}

export async function waitForStructureReady(
  viewer,
  { maxTries = 12, intervalMs = 150 } = {},
) {
  for (let attempt = 0; attempt < maxTries; attempt++) {
    if (getLoadedStructureCount(viewer) > 0) return true;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  return getLoadedStructureCount(viewer) > 0;
}

export async function applySelectionWithRetry(
  viewer,
  {
    data,
    nonSelectedColor,
    focus = false,
    keepRepresentations = true,
    structureNumber,
    maxTries = 12,
    intervalMs = 150,
  },
) {
  if (!viewer?.visual?.select) return false;
  if (!Array.isArray(data) || data.length === 0) return false;

  await waitForStructureReady(viewer, { maxTries, intervalMs });

  for (let attempt = 0; attempt < maxTries; attempt++) {
    try {
      await viewer.visual.select({
        data,
        nonSelectedColor,
        focus,
        keepRepresentations,
        structureNumber,
      });
      return true;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
  }

  return false;
}

export async function highlightResidues(
  viewer,
  { data, color, focus = true, structureNumber } = {},
) {
  if (!viewer?.visual?.highlight) return;
  if (!Array.isArray(data) || data.length === 0) return;

  return await viewer.visual.highlight({ data, color, focus, structureNumber });
}

export async function clearHighlight(viewer) {
  if (!viewer?.visual?.clearHighlight) return;
  return await viewer.visual.clearHighlight();
}

export function molstarFormatFromPskitFormat(format) {
  const f = String(format || "")
    .toLowerCase()
    .trim();
  if (f === "cif" || f === "mmcif") return "mmcif";
  if (f === "pdb") return "pdb";
  return f || "pdb";
}

export function createBlobUrlFromBytes(
  bytes,
  { mime = "application/octet-stream" } = {},
) {
  if (!bytes) throw new Error("bytes is required");
  const blob = new Blob([bytes], { type: mime });
  return URL.createObjectURL(blob);
}
