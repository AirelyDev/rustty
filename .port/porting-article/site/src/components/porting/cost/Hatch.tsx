/** The copper hatch that marks modeled (not measured) cost, as an SVG pattern with the given id. */
export default function Hatch({ id }: { id: string }) {
  return (
    <defs>
      <pattern
        id={id}
        width={6}
        height={6}
        patternUnits="userSpaceOnUse"
        patternTransform="rotate(45)"
      >
        <rect width={6} height={6} fill="#d9793422" />
        <line
          x1={0}
          y1={0}
          x2={0}
          y2={6}
          stroke="#d97934"
          strokeWidth={2.2}
          strokeOpacity={0.75}
        />
      </pattern>
    </defs>
  );
}
