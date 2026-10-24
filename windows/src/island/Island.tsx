// Placeholder until the shell spec lands: a pill that grows on hover, to
// prove the window, pass-through and hit-rect plumbing end to end.
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { bridge } from "../shared/bridge";

export function Island() {
  const ref = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let frame = 0;
    const report = () => {
      const el = ref.current;
      if (el) {
        const r = el.getBoundingClientRect();
        void bridge.setHitRects([{ x: r.x, y: r.y, width: r.width, height: r.height, radius: 0 }]);
      }
      frame = requestAnimationFrame(report);
    };
    frame = requestAnimationFrame(report);
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div style={{ display: "flex", justifyContent: "center" }}>
      <motion.div
        ref={ref}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        animate={{ width: open ? 420 : 200, height: open ? 160 : 32, borderBottomLeftRadius: open ? 32 : 14, borderBottomRightRadius: open ? 32 : 14 }}
        transition={{ type: "spring", stiffness: 300, damping: 26 }}
        style={{ background: "#000" }}
      />
    </div>
  );
}
