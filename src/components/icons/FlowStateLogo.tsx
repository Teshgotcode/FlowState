// FlowState wordmark: SVG text so it re-renders crisply at any width.
// Metrics measured for SF Pro Bold at 100px ("flowstate" ≈ 409, dot ≈ 24);
// viewBox carries ~15% slack for wider system-ui fonts on other platforms.
const FlowStateLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 500 130"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* eslint-disable i18next/no-literal-string -- brand wordmark, not localizable copy */}
      <text
        x={250}
        y={100}
        textAnchor="middle"
        fontFamily="system-ui"
        fontWeight={700}
        letterSpacing="-0.02em"
        fontSize={100}
      >
        <tspan fill="var(--color-logo-primary)">flow</tspan>
        <tspan fill="var(--color-logo-stroke)">state</tspan>
        <tspan fill="var(--color-logo-primary)">.</tspan>
      </text>
      {/* eslint-enable i18next/no-literal-string */}
    </svg>
  );
};

export default FlowStateLogo;
