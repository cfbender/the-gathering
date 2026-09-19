defmodule TheGathering.Catalog.Gzip do
  @moduledoc false

  @chunk_size 64 * 1024

  def lines(path) do
    Stream.resource(
      fn -> open(path) end,
      &next/1,
      &close/1
    )
  end

  defp open(path) do
    {:ok, file} = File.open(path, [:read, :binary, :raw])
    zlib = :zlib.open()
    :ok = :zlib.inflateInit(zlib, 31)
    %{file: file, zlib: zlib, buffer: "", pending: [], done: false}
  end

  defp next(%{pending: [line | rest]} = state), do: {[line], %{state | pending: rest}}
  defp next(%{done: true} = state), do: {:halt, state}

  defp next(state) do
    case IO.binread(state.file, @chunk_size) do
      data when is_binary(data) ->
        inflated = state.zlib |> :zlib.inflate(data) |> IO.iodata_to_binary()
        {lines, buffer} = split_lines(state.buffer <> inflated)
        next(%{state | pending: lines, buffer: buffer})

      :eof ->
        pending = if state.buffer == "", do: [], else: [state.buffer]
        next(%{state | pending: pending, buffer: "", done: true})

      {:error, reason} ->
        raise File.Error, reason: reason, action: "stream", path: "bulk data"
    end
  end

  defp split_lines(data) do
    parts = :binary.split(data, "\n", [:global])
    {Enum.drop(parts, -1), List.last(parts)}
  end

  defp close(state) do
    :zlib.inflateEnd(state.zlib)
    :zlib.close(state.zlib)
    File.close(state.file)
  end
end
