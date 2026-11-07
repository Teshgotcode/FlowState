// The FlowState mark: two pill bars on a shared baseline, unequal height.
// Geometry lives on a 24 grid so it stays crisp from 16 px to the app icon.
import React, { useEffect, useState } from "react";

type Frame = {
  /** Offset from the bar's resting position, in viewBox units. */
  dx: number;
  dy: number;
  /** Vertical scale: 1 is the resting bar, ~0.45 an eye. */
  sy: number;
};

const REST: [Frame, Frame] = [
  { dx: 0, dy: 0, sy: 1 },
  { dx: 0, dy: 0, sy: 1 },
];

// Both bars shrink and slide until they sit level: the strokes become eyes.
const EYES: [Frame, Frame] = [
  { dx: 1, dy: -2.5, sy: 0.4516 },
  { dx: -1.2, dy: -4.9, sy: 0.7368 },
];

const BLINK: [Frame, Frame] = [
  { ...EYES[0], sy: EYES[0].sy * 0.12 },
  { ...EYES[1], sy: EYES[1].sy * 0.12 },
];

const TIMING = 420;
const EASE = "cubic-bezier(0.2, 0.7, 0.2, 1)";

const useEyes = (enabled: boolean) => {
  const [frame, setFrame] = useState<[Frame, Frame]>(REST);

  useEffect(() => {
    if (!enabled) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    const timers = [
      setTimeout(() => setFrame(EYES), 260),
      setTimeout(() => setFrame(BLINK), 760),
      setTimeout(() => setFrame(EYES), 900),
      setTimeout(() => setFrame(REST), 1700),
    ];
    return () => timers.forEach(clearTimeout);
  }, [enabled]);

  return frame;
};

const bar = ({ dx, dy, sy }: Frame): React.CSSProperties => ({
  transform: `translate(${dx}px, ${dy}px) scaleY(${sy})`,
  transformBox: "fill-box",
  transformOrigin: "center",
  transition: `transform ${TIMING}ms ${EASE}`,
});

const FlowStateMark = ({
  width = 24,
  height = 24,
  className,
  eyesOnMount = false,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
  /** Blink once on mount: the two strokes turn into eyes and back. */
  eyesOnMount?: boolean;
}) => {
  const [left, right] = useEyes(eyesOnMount);

  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      className={className}
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect
        x="4.6"
        y="6.8"
        width="5.6"
        height="12.4"
        rx="2.8"
        style={bar(left)}
      />
      <rect
        x="14"
        y="11.6"
        width="5.6"
        height="7.6"
        rx="2.8"
        style={bar(right)}
      />
    </svg>
  );
};

export default FlowStateMark;
