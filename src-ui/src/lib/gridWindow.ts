export function gridWindow(count: number, width: number, top: number, height: number, minCell = 160, gap = 4) {
  const columns = Math.max(1, Math.floor((width + gap) / (minCell + gap)));
  const pitch = (Math.max(1, width) - gap * (columns - 1)) / columns + gap;
  const rows = Math.ceil(count / columns);
  const first = Math.min(rows, Math.max(0, Math.floor(top / pitch) - 3));
  const last = Math.min(rows, Math.max(first, Math.ceil((top + height) / pitch) + 3));
  return { columns, start: first * columns, end: Math.min(count, last * columns),
    paddingTop: first * pitch, paddingBottom: Math.max(0, (rows - last) * pitch) };
}
