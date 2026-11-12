export interface TreeInput {
  id: string;
  value: number;
}
export interface TreeRect {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  value: number;
}

/** Squarified treemap (Bruls, Huizing, van Wijk). Items are laid out in descending value order. */
export function squarify(items: TreeInput[], width: number, height: number): TreeRect[] {
  const total = items.reduce((a, b) => a + Math.max(0, b.value), 0);
  if (total <= 0 || width <= 0 || height <= 0) return [];
  const area = width * height;
  const sorted = [...items]
    .filter((i) => i.value > 0)
    .sort((a, b) => b.value - a.value)
    .map((i) => ({ ...i, area: (i.value / total) * area }));
  const out: TreeRect[] = [];
  let x = 0,
    y = 0,
    w = width,
    h = height;
  let row: typeof sorted = [];

  const worst = (r: typeof sorted, side: number) => {
    const s = r.reduce((a, b) => a + b.area, 0);
    if (!s) return Infinity;
    const mx = Math.max(...r.map((i) => i.area)),
      mn = Math.min(...r.map((i) => i.area));
    return Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn));
  };

  const layoutRow = (r: typeof sorted) => {
    const s = r.reduce((a, b) => a + b.area, 0);
    const horizontal = w >= h; // lay the row along the shorter side
    if (horizontal) {
      const rw = s / h;
      let cy = y;
      for (const i of r) {
        const rh = i.area / rw;
        out.push({ id: i.id, x, y: cy, w: rw, h: rh, value: i.value });
        cy += rh;
      }
      x += rw;
      w -= rw;
    } else {
      const rh = s / w;
      let cx = x;
      for (const i of r) {
        const rw = i.area / rh;
        out.push({ id: i.id, x: cx, y, w: rw, h: rh, value: i.value });
        cx += rw;
      }
      y += rh;
      h -= rh;
    }
  };

  for (const item of sorted) {
    const side = Math.min(w, h);
    if (row.length === 0 || worst([...row, item], side) <= worst(row, side)) row.push(item);
    else {
      layoutRow(row);
      row = [item];
    }
  }
  if (row.length) layoutRow(row);
  return out;
}
