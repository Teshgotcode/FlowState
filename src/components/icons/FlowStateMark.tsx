// The FlowState mark: two pill bars on a shared baseline, unequal height.
// Geometry lives on a 24 grid so it stays crisp from 16 px to the app icon.
const FlowStateMark = ({
  width = 24,
  height = 24,
  className,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => (
  <svg
    width={width}
    height={height}
    viewBox="0 0 24 24"
    className={className}
    fill="currentColor"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <rect x="4.6" y="6.8" width="5.6" height="12.4" rx="2.8" />
    <rect x="14" y="11.6" width="5.6" height="7.6" rx="2.8" />
  </svg>
);

export default FlowStateMark;
