defmodule TheGathering.CardId.Corrections do
  @moduledoc """
  Human-labelled click crops. JPEGs stay native; the offline importer creates card.png.
  Writes are serialized on this single-container application, with the append-only label
  log as the commit point. Repeated capture/label submissions are idempotent.
  """
  alias TheGathering.CardId
  alias TheGathering.Catalog.PrintingId

  @uuid ~r/\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z/
  @fields ~w(capture_id label click quad up_vote bundle_version top1 similarity margin)

  def directory, do: Path.join(CardId.bundle_dir(), "corrections")

  def save(params, user_id) do
    with {:ok, jpeg} <- validate(params) do
      :global.trans({__MODULE__, self()}, fn -> persist(params, jpeg, user_id) end)
    end
  end

  def page(cursor) when is_integer(cursor) and cursor >= 0 do
    rows = labels() |> Stream.drop(cursor) |> Enum.take(50)
    %{corrections: rows, cursor: cursor + length(rows), has_more: length(rows) == 50}
  end

  def crop_path(id) do
    path = Path.join([directory(), id, "crop.jpg"])

    if uuid?(id) and File.regular?(path),
      do: {:ok, path},
      else: {:error, :not_found}
  end

  defp validate(%{"image" => "data:image/jpeg;base64," <> encoded} = p)
       when byte_size(encoded) <= 190_000 do
    with true <- uuid?(p["capture_id"]),
         {:ok, _, _} <- PrintingId.parse(p["label"]),
         true <- point?(p["click"], 0, 640),
         true <- quad?(p["quad"]),
         true <- optional_number?(p["up_vote"], 0, 2),
         true <- optional_number?(p["similarity"], -2, 2),
         true <- optional_number?(p["margin"], 0, 4),
         true <- is_nil(p["top1"]) or match?({:ok, _, _}, PrintingId.parse(p["top1"])),
         true <- is_binary(p["bundle_version"]) and byte_size(p["bundle_version"]) <= 120,
         {:ok, <<255, 216, rest::binary>> = jpeg} <- Base.decode64(encoded),
         true <- :binary.part(jpeg, byte_size(jpeg) - 2, 2) == <<255, 217>>,
         {width, height} <- jpeg_size(rest),
         [x, y] <- p["click"],
         true <- width in 1..640 and height in 1..640 and x <= width and y <= height do
      {:ok, jpeg}
    else
      _ -> {:error, :bad_request}
    end
  end

  defp validate(_), do: {:error, :bad_request}
  defp uuid?(id), do: is_binary(id) and Regex.match?(@uuid, id)
  defp number?(n, low, high), do: is_number(n) and n >= low and n <= high
  defp optional_number?(nil, _, _), do: true
  defp optional_number?(n, low, high), do: number?(n, low, high)
  defp point?([x, y], low, high), do: number?(x, low, high) and number?(y, low, high)
  defp point?(_, _, _), do: false
  defp quad?(nil), do: true
  defp quad?(q) when is_list(q), do: length(q) == 4 and Enum.all?(q, &point?(&1, -2048, 2048))
  defp quad?(_), do: false

  # Read baseline/progressive JPEG frame dimensions without decoding pixels in Phoenix.
  # Full image validation and the warp happen in the bounded offline importer.
  defp jpeg_size(<<255, marker, _length::16, 8, height::16, width::16, _::binary>>)
       when marker in [192, 194],
       do: {width, height}

  defp jpeg_size(<<255, marker, length::16, rest::binary>>)
       when marker not in [216, 217, 218] and length >= 2 and byte_size(rest) >= length - 2 do
    rest |> binary_part(length - 2, byte_size(rest) - length + 2) |> jpeg_size()
  end

  defp jpeg_size(_), do: :invalid

  defp persist(params, jpeg, user_id) do
    id = params["capture_id"]
    dir = Path.join(directory(), id)
    owner_path = Path.join(dir, "owner")
    owner = Integer.to_string(user_id)

    case File.read(owner_path) do
      {:ok, other} when other != owner -> {:error, :forbidden}
      _ -> write_label(params, jpeg, dir, owner_path, owner)
    end
  end

  defp write_label(params, jpeg, dir, owner_path, owner) do
    row =
      params
      |> Map.take(@fields)
      |> Map.put("split", split_for(params["capture_id"]))
      |> Map.put("source", "webcam-table")

    latest_path = Path.join(dir, "label.json")
    encoded = Jason.encode!(row)

    if File.read(latest_path) != {:ok, encoded} do
      File.mkdir_p!(dir)
      File.write!(owner_path, owner)
      # Never overwrite the image of an existing capture, including after a relabel.
      unless File.exists?(Path.join(dir, "crop.jpg")),
        do: File.write!(Path.join(dir, "crop.jpg"), jpeg)

      File.write!(Path.join(directory(), "labels.jsonl"), encoded <> "\n", [:append])
      File.write!(latest_path, encoded)
    end

    {:ok, %{capture_id: params["capture_id"]}}
  end

  defp split_for(id) do
    hash = :crypto.hash(:sha, id) |> :binary.decode_unsigned()
    if rem(hash, 5) == 0, do: "eval", else: "train"
  end

  defp labels do
    path = Path.join(directory(), "labels.jsonl")

    if File.exists?(path),
      do: path |> File.stream!() |> Stream.map(&Jason.decode!/1),
      else: []
  end
end
