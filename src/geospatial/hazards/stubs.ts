/** Experimental multi-hazard data stubs — feeds / STAC only, no fake physics. */

export type HazardStubId = "flood" | "quake" | "fire" | "landslide" | "tsunami";

export type HazardStubKind = "simulate" | "experimental_stub";

export interface HazardStubLink {
  label: string;
  href: string;
  /** Short source tag shown on the card. */
  source: string;
}

export interface HazardStubDef {
  id: HazardStubId;
  label: string;
  kind: HazardStubKind;
  /** Layer ids in defaults.ts that this stub owns (hook status). */
  layerIds: string[];
  blurb: string;
  links: HazardStubLink[];
}

/**
 * Flood is the only simulated physics path. Quake / fire / landslide / tsunami
 * are Experimental feed/STAC cards — never solvers.
 */
export const HAZARD_STUBS: HazardStubDef[] = [
  {
    id: "flood",
    label: "Flood",
    kind: "simulate",
    layerIds: ["flood_depth", "flood_velocity", "flood_hazard", "flood_uncertainty"],
    blurb:
      "Simulate inundation (soft preview / HAND / ANUGA when installed). Data layers stay overlays — not a substitute for calibrated runs.",
    links: [
      {
        label: "Earth Search STAC",
        href: "https://earth-search.aws.element84.com/v1",
        source: "STAC",
      },
      {
        label: "USGS Water Services",
        href: "https://waterservices.usgs.gov/",
        source: "USGS",
      },
    ],
  },
  {
    id: "quake",
    label: "Earthquake",
    kind: "experimental_stub",
    layerIds: ["hazard_quake"],
    blurb:
      "Feed / catalog stub only. No ground-motion or rupture physics in this release.",
    links: [
      {
        label: "USGS Earthquake Hazards",
        href: "https://earthquake.usgs.gov/earthquakes/feed/",
        source: "USGS",
      },
      {
        label: "GDACS disasters",
        href: "https://www.gdacs.org/",
        source: "GDACS",
      },
    ],
  },
  {
    id: "fire",
    label: "Wildfire",
    kind: "experimental_stub",
    layerIds: ["hazard_fire"],
    blurb:
      "External alert / imagery links only. No fire-spread solver or authoritative burn model.",
    links: [
      {
        label: "GDACS wildfire",
        href: "https://www.gdacs.org/",
        source: "GDACS",
      },
      {
        label: "Earth Search STAC",
        href: "https://earth-search.aws.element84.com/v1",
        source: "STAC",
      },
    ],
  },
  {
    id: "landslide",
    label: "Landslide",
    kind: "experimental_stub",
    layerIds: ["hazard_landslide"],
    blurb:
      "Catalog / feed card only. No slope-failure physics or susceptibility solver.",
    links: [
      {
        label: "GDACS",
        href: "https://www.gdacs.org/",
        source: "GDACS",
      },
      {
        label: "USGS Landslide Hazards",
        href: "https://www.usgs.gov/programs/landslide-hazards",
        source: "USGS",
      },
    ],
  },
  {
    id: "tsunami",
    label: "Tsunami",
    kind: "experimental_stub",
    layerIds: ["hazard_tsunami"],
    blurb:
      "Alert / feed stub only. No wave-propagation or inundation physics beyond flood tools.",
    links: [
      {
        label: "GDACS tsunami",
        href: "https://www.gdacs.org/",
        source: "GDACS",
      },
      {
        label: "USGS Earthquake feed",
        href: "https://earthquake.usgs.gov/earthquakes/feed/",
        source: "USGS",
      },
    ],
  },
];

export const HAZARD_NON_CLAIM =
  "Only flood has simulated physics in v0.10. Earthquake, wildfire, landslide, and tsunami are Experimental data stubs (GDACS / USGS feeds or STAC links) — not solvers and not authoritative.";
