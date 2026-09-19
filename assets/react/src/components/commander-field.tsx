interface CommanderFieldProps {
  value: string
  onChange: (value: string) => void
  required?: boolean
}

/** Plain-text seam for the catalog thread to replace with CardSearch. */
export function CommanderField({ value, onChange, required }: CommanderFieldProps) {
  return (
    <label className="form-control flex-1">
      <span className="label-text mb-1 text-xs font-medium">Commander</span>
      <input
        className="input input-bordered input-sm w-full"
        placeholder="Commander name"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        required={required}
      />
    </label>
  )
}
