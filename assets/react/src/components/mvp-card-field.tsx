interface MvpCardFieldProps {
  value: string
  onChange: (value: string) => void
}

/** Plain-text seam for the catalog thread to replace with CardSearch. */
export function MvpCardField({ value, onChange }: MvpCardFieldProps) {
  return (
    <label className="form-control">
      <span className="label-text mb-1 text-xs font-medium">MVP card (optional)</span>
      <input
        className="input input-bordered input-sm w-full"
        placeholder="The card that mattered"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  )
}
