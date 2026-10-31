// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n";

import { contentInsets, expandedWidth, fitContent, headerMetrics, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { useIsland } from "../island/store";
import { Preview } from "./Preview";
import { demoInfo } from "./demo";

/** The screenshot uses the same contour, fit and quick-access bubbles as pages. */
export function CapturePresentation({ id, geometry, size }: { id: string; geometry: Geometry; size: Size }) {
  useLocale();
  const header = headerMetrics(geometry, true);
  const insets = contentInsets(geometry, size);
  const reference = { ...geometry, input: { ...geometry.input, layout: "spacious" as const } };
  const width = expandedWidth(reference) - 56;
  const height = 264 + header.pageTopInset - header.headerTopInset;
  const fit = fitContent(
    { width: size.width - 2 * insets.horizontal, height: size.height - header.headerTopInset - insets.bottom },
    { width, height },
  );
  return (
    <div
      className="n-expanded n-capture-preview"
      data-screenshot-id={id}
      style={{ width: size.width, height: size.height }}
    >
      <div
        className="n-expanded-layout"
        style={{
          width,
          height,
          left: insets.horizontal + fit.x,
          top: header.headerTopInset + fit.y,
          transform: `scale(${fit.scale})`,
        }}
      >
        <Preview
          id={id}
          embedded
          initialInfo={id === "demo" ? demoInfo : undefined}
          onClosed={() => useIsland.getState().dismissCapturePreview(id)}
        />
      </div>
    </div>
  );
}
