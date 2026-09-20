export interface ColorWheelSlice {
  id: string
  games: number
  percentage: number
  startAngle: number
  endAngle: number
}

export function colorWheelSlices<T extends { id: string; games: number }>(
  rows: T[],
): ColorWheelSlice[] {
  const total = rows.reduce((sum, row) => sum + row.games, 0)
  let angle = -90

  return rows.map((row) => {
    const percentage = total === 0 ? 0 : (row.games / total) * 100
    const startAngle = angle
    angle += percentage * 3.6
    return { id: row.id, games: row.games, percentage, startAngle, endAngle: angle }
  })
}

function polarPoint(angle: number, radius: number) {
  const radians = (angle * Math.PI) / 180
  return { x: 100 + Math.cos(radians) * radius, y: 100 + Math.sin(radians) * radius }
}

export function donutSlicePath(startAngle: number, endAngle: number, outer = 88, inner = 48) {
  const sweep = endAngle - startAngle
  if (sweep <= 0) return ""

  if (sweep >= 359.999) {
    const outerStart = polarPoint(startAngle, outer)
    const outerMid = polarPoint(startAngle + 180, outer)
    const innerStart = polarPoint(startAngle, inner)
    const innerMid = polarPoint(startAngle + 180, inner)
    return `M ${outerStart.x} ${outerStart.y} A ${outer} ${outer} 0 1 1 ${outerMid.x} ${outerMid.y} A ${outer} ${outer} 0 1 1 ${outerStart.x} ${outerStart.y} L ${innerStart.x} ${innerStart.y} A ${inner} ${inner} 0 1 0 ${innerMid.x} ${innerMid.y} A ${inner} ${inner} 0 1 0 ${innerStart.x} ${innerStart.y} Z`
  }

  const outerStart = polarPoint(startAngle, outer)
  const outerEnd = polarPoint(endAngle, outer)
  const innerEnd = polarPoint(endAngle, inner)
  const innerStart = polarPoint(startAngle, inner)
  const largeArc = sweep > 180 ? 1 : 0
  return `M ${outerStart.x} ${outerStart.y} A ${outer} ${outer} 0 ${largeArc} 1 ${outerEnd.x} ${outerEnd.y} L ${innerEnd.x} ${innerEnd.y} A ${inner} ${inner} 0 ${largeArc} 0 ${innerStart.x} ${innerStart.y} Z`
}
