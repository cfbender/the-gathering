export interface GridCell {
  /** 1-based, ready for CSS `grid-row`. */
  row: number
  /** 1-based, ready for CSS `grid-column`. */
  column: number
}

export interface CameraGridLayout {
  columns: number
  rows: number
  /** One cell per seat, in turn order. */
  cells: GridCell[]
}

function perimeter(columns: number, rows: number) {
  return rows === 1 || columns === 1 ? columns * rows : 2 * (columns + rows) - 4
}

/** Seats `count` cameras around the edge of a near-square grid, clockwise from the top-left,
 * so turn order reads the way players sit around a table. Adds a column when the interior
 * would otherwise be needed. */
export function cameraGridLayout(count: number): CameraGridLayout {
  let columns = Math.max(1, Math.ceil(Math.sqrt(count)))
  let rows = Math.max(1, Math.ceil(count / columns))
  while (perimeter(columns, rows) < count) {
    columns += 1
    rows = Math.ceil(count / columns)
  }

  const ring: GridCell[] = []
  for (let column = 1; column <= columns; column++) ring.push({ row: 1, column })
  for (let row = 2; row <= rows; row++) ring.push({ row, column: columns })
  if (rows > 1)
    for (let column = columns - 1; column >= 1; column--) ring.push({ row: rows, column })
  if (columns > 1) for (let row = rows - 1; row >= 2; row--) ring.push({ row, column: 1 })

  return { columns, rows, cells: ring.slice(0, count) }
}
