import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type GeoReference } from "../lib/ipc";
import { useStore } from "../state/store";
import {
  localizeCameraVps,
  type CameraIntrinsics,
  type QueryKeypoint2D,
  type SplatAnchor3D,
  type VpsPoseResult,
  type VpsSubmap,
} from "./vps";


/**
 * Visual Positioning System panel.
 *
 * What this does: takes a **submap bundle** (3-D splat anchors with descriptors + the submap's
 * WGS84 origin) and a **query bundle** (2-D keypoints with descriptors, intrinsics, and optionally
 * a coarse GNSS prior) and runs the four-stage localiser in `vps.ts`: retrieval → 2-D/3-D anchor
 * matching → PnP + RANSAC + LM → ENU/WGS84 registration.
 *
 * Where the descriptors come from is deliberately not faked here. SuperPoint/LightGlue-class
 * extraction is an external sidecar (`tools/sidecars/lightglue`); this panel consumes its JSON. The
 * "Run self-test" action builds a synthetic submap and query in memory so the pipeline can be
 * exercised without a capture — it is labelled synthetic in the result, because a self-test proves
 * the algebra, not that a photo of a real place was localised.
 */

interface SubmapBundle {
  submaps: VpsSubmap[];
}

interface QueryBundle {
  globalDescriptor: number[];
  keypoints: QueryKeypoint2D[];
  intrinsics: CameraIntrinsics;
  domain?: "ground" | "geospatial" | "hybrid";
  priorWgs84?: [number, number, number];
  priorCenterEnu?: [number, number, number];
}

function parseSubmaps(text: string): SubmapBundle {
  const raw = JSON.parse(text) as Partial<SubmapBundle> & Partial<VpsSubmap>;
  const submaps = raw.submaps ?? (raw.anchors ? [raw as VpsSubmap] : null);
  if (!submaps || !Array.isArray(submaps) || submaps.length === 0) {
    throw new Error("submap bundle must be { submaps: [...] } or a single submap with anchors");
  }
  for (const sm of submaps) {
    if (!sm.originWgs84 || !Array.isArray(sm.anchors)) {
      throw new Error("each submap needs originWgs84 and anchors");
    }
  }
  return { submaps };
}

function parseQuery(text: string): QueryBundle {
  const q = JSON.parse(text) as Partial<QueryBundle>;
  if (!Array.isArray(q.keypoints) || q.keypoints.length === 0) {
    throw new Error("query bundle needs keypoints[{u,v,descriptor}]");
  }
  if (!q.intrinsics || typeof q.intrinsics.fx !== "number") {
    throw new Error("query bundle needs intrinsics{width,height,fx,fy,cx,cy}");
  }
  return {
    globalDescriptor: q.globalDescriptor ?? [],
    keypoints: q.keypoints,
    intrinsics: q.intrinsics,
    domain: q.domain ?? "hybrid",
    priorWgs84: q.priorWgs84,
    priorCenterEnu: q.priorCenterEnu,
  };
}

/** Deterministic synthetic submap + query: 8 anchors seen by a camera 18 m away. */
function buildSelfTest(originWgs84: [number, number, number]): {
  submap: VpsSubmap;
  query: QueryBundle;
  truthEnu: [number, number, number];
} {
  const intrinsics: CameraIntrinsics = { width: 1280, height: 720, fx: 920, fy: 920, cx: 640, cy: 360 };
  const truthEnu: [number, number, number] = [4, -18, 1.8];
  // Camera looking north, image x = east, image y = down.
  const rotation = [1, 0, 0, 0, 0, -1, 0, 1, 0];
  const coords: [number, number, number][] = [
    [-4, 10, 2.5],
    [10, 12, 3.2],
    [-2.5, 22, 6],
    [11, 25, 7.5],
    [3, 15, 1.2],
    [6.5, 18, 4.8],
    [-5.5, 16, 5.1],
    [8, 28, 2],
  ];
  const anchors: SplatAnchor3D[] = [];
  const keypoints: QueryKeypoint2D[] = [];
  coords.forEach((p, i) => {
    const descriptor = new Float32Array(16);
    descriptor[i % 16] = 1;
    descriptor[(i * 3 + 1) % 16] = 0.5;
    anchors.push({
      anchorId: `anchor-${i}`,
      positionEnu: p,
      descriptor,
      saliency: 0.9,
      domain: "ground",
    });
    // Project with the same convention the solver uses.
    const d: [number, number, number] = [p[0] - truthEnu[0], p[1] - truthEnu[1], p[2] - truthEnu[2]];
    const x = rotation[0] * d[0] + rotation[1] * d[1] + rotation[2] * d[2];
    const y = rotation[3] * d[0] + rotation[4] * d[1] + rotation[5] * d[2];
    const z = rotation[6] * d[0] + rotation[7] * d[1] + rotation[8] * d[2];
    keypoints.push({
      u: intrinsics.cx + (x * intrinsics.fx) / z,
      v: intrinsics.cy + (y * intrinsics.fy) / z,
      descriptor: Array.from(descriptor),
    });
  });
  return {
    submap: {
      submapId: "self-test-submap",
      originWgs84,
      globalDescriptor: [0.8, 0.5, 0.2, 0.1],
      anchors,
    },
    query: {
      globalDescriptor: [0.82, 0.48, 0.21, 0.09],
      keypoints,
      intrinsics,
      domain: "ground",
      priorCenterEnu: [3.2, -16.5, 2],
    },
    truthEnu,
  };
}

function fmt(n: number, digits = 2): string {
  return Number.isFinite(n) ? n.toFixed(digits) : "—";
}

export default function VpsPanel() {
  const workspace = useStore((s) => s.workspace);
  const [geoReference, setGeoReference] = useState<GeoReference | null>(null);
  const [submapText, setSubmapText] = useState("");
  const [queryText, setQueryText] = useState("");
  const [result, setResult] = useState<VpsPoseResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);

  // The submap's ENU origin is the project's georeferenced origin, so a pose lands in the same
  // frame the rest of the geospatial suite uses.
  useEffect(() => {
    let cancelled = false;
    // No synchronous setState here: the async result (including "no workspace") lands in a
    // callback, so this effect cannot cascade a render.
    void (workspace
      ? api.getGeoReference(workspace)
      : Promise.resolve<GeoReference | null>(null)
    )
      .then((ref) => {
        if (!cancelled) setGeoReference(ref);
      })
      .catch(() => {
        if (!cancelled) setGeoReference(null);
      });
    return () => {
      cancelled = true;
    };
  }, [workspace]);

  const originWgs84 = useMemo<[number, number, number]>(() => {
    const o = geoReference?.localOrigin;
    if (o && Number.isFinite(o[0]) && Number.isFinite(o[1])) {
      return [o[0], o[1], o[2] ?? 0];
    }
    return [0, 0, 0];
  }, [geoReference]);

  const run = (submaps: VpsSubmap[], query: QueryBundle, label: string) => {
    setError(null);
    setSavedPath(null);
    try {
      const pose = localizeCameraVps(submaps, query);
      setResult(pose);
      setNote(label);
      if (!pose.success) {
        setError("localisation failed: fewer than 4 consistent anchors. Add a prior or a richer submap.");
      }
    } catch (err) {
      setResult(null);
      setError(String(err));
    }
  };

  const runFromFiles = () => {
    try {
      const { submaps } = parseSubmaps(submapText);
      run(submaps, parseQuery(queryText), "from files");
    } catch (err) {
      setResult(null);
      setError(String(err));
    }
  };

  const runSelfTest = () => {
    const test = buildSelfTest(originWgs84);
    run([test.submap], test.query, "synthetic self-test (not a real localisation)");
  };

  const save = async () => {
    if (!result) return;
    setError(null);
    try {
      const path = await api.saveVpsLocalization({
        submapId: result.submapId,
        domain: result.domainUsed,
        positionEnu: result.positionEnu,
        positionWgs84: result.positionWgs84,
        headingDeg: result.headingDeg,
        pitchDeg: result.pitchDeg,
        rollDeg: result.rollDeg,
        inlierCount: result.inlierCount,
        medianReprojectionErrorPx: result.medianReprojectionErrorPx,
        sigmaHorizontalM: result.hdopM,
        sigmaVerticalM: result.vdopM,
        source: note ?? "unknown",
      });
      setSavedPath(path);
    } catch (err) {
      setError(String(err));
    }
  };

  const load = async (kind: "submap" | "query") => {
    try {
      const picked = await open({
        title: kind === "submap" ? "Submap bundle (JSON)" : "Query bundle (JSON)",
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      const text = await api.readTextFile(path);
      if (kind === "submap") setSubmapText(text);
      else setQueryText(text);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="flex flex-col gap-2 text-xs" data-testid="vps-panel">
      <p className="text-[11px] leading-snug text-ink-dim">
        Localise a query image inside a splat submap. Anchors and descriptors come from the
        LightGlue sidecar export; the panel runs retrieval, matching, PnP and ENU/WGS84
        registration.
      </p>

      <div className="flex flex-wrap gap-1.5">
        <button type="button" className="btn btn-ghost px-2 py-1 text-[11px]" onClick={() => void load("submap")}>
          Load submap
        </button>
        <button type="button" className="btn btn-ghost px-2 py-1 text-[11px]" onClick={() => void load("query")}>
          Load query
        </button>
        <button
          type="button"
          className="btn px-2 py-1 text-[11px]"
          onClick={runFromFiles}
          disabled={!submapText || !queryText}
        >
          Localise
        </button>
        <button type="button" className="btn btn-ghost px-2 py-1 text-[11px]" onClick={runSelfTest}>
          Run self-test
        </button>
      </div>

      {submapText ? (
        <span className="text-[10px] text-ink-dim">
          submap bundle loaded ({submapText.length.toLocaleString()} chars)
        </span>
      ) : null}
      {queryText ? (
        <span className="text-[10px] text-ink-dim">
          query bundle loaded ({queryText.length.toLocaleString()} chars)
        </span>
      ) : null}

      {error ? (
        <p role="alert" className="rounded border border-critical/60 bg-critical/10 p-1.5 text-[11px] text-ink">
          {error}
        </p>
      ) : null}

      {result ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-2 gap-y-1 text-[11px]">
          <dt className="text-ink-dim">confidence</dt>
          <dd className="tabular-nums text-ink">{result.confidence}</dd>
          <dt className="text-ink-dim">inliers</dt>
          <dd className="tabular-nums text-ink">
            {result.inlierCount} / {result.totalMatches}
          </dd>
          <dt className="text-ink-dim">reproj. median</dt>
          <dd className="tabular-nums text-ink">{fmt(result.medianReprojectionErrorPx)} px</dd>
          <dt className="text-ink-dim">ENU</dt>
          <dd className="tabular-nums text-ink">
            {result.positionEnu.map((v) => fmt(v, 2)).join(", ")} m
          </dd>
          <dt className="text-ink-dim">WGS84</dt>
          <dd className="tabular-nums text-ink">
            {result.positionWgs84[0].toFixed(7)}, {result.positionWgs84[1].toFixed(7)},{" "}
            {fmt(result.positionWgs84[2])} m
          </dd>
          <dt className="text-ink-dim">heading / pitch</dt>
          <dd className="tabular-nums text-ink">
            {fmt(result.headingDeg, 1)}° / {fmt(result.pitchDeg, 1)}°
          </dd>
          <dt className="text-ink-dim">σ horizontal / vertical</dt>
          <dd className="tabular-nums text-ink">
            {fmt(result.hdopM)} / {fmt(result.vdopM)} m
          </dd>
          {note ? (
            <>
              <dt className="text-ink-dim">source</dt>
              <dd className="text-ink">{note}</dd>
            </>
          ) : null}
        </dl>
      ) : null}

      {result?.success ? (
        <div className="flex items-center gap-2">
          <button type="button" className="btn btn-ghost px-2 py-1 text-[11px]" onClick={() => void save()}>
            Save fix
          </button>
          {savedPath ? <span className="text-[10px] text-ink-dim">saved: {savedPath}</span> : null}
        </div>
      ) : null}

      {geoReference ? (
        <span className="text-[10px] text-ink-dim">
          submap origin from project GCP: {originWgs84[0].toFixed(6)}, {originWgs84[1].toFixed(6)}
        </span>
      ) : (
        <span className="text-[10px] text-ink-dim">
          no project GCP yet — self-test uses an arbitrary local origin
        </span>
      )}
      <span className="text-[10px] text-ink-dim">
        σ is a first-order bound from reprojection error and landmark depth, not a calibrated
        covariance.
      </span>
    </div>
  );
}

