import { useLayoutEffect, useMemo, useRef } from "react";

/** Async navigation belongs to one mounted layout and one user intent. */
export function useNavigationIntent() {
  const epoch = useRef(0);
  const mounted = useRef(false);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; epoch.current++; };
  }, []);
  return useMemo(() => {
    const capture = () => {
      const mine = epoch.current;
      return () => mounted.current && epoch.current === mine;
    };
    const invalidate = () => { epoch.current++; };
    return { capture, invalidate, begin: () => { invalidate(); return capture(); } };
  }, []);
}
