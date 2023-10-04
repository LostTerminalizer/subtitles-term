while getopts i:o:m:n: flag
do
    case "${flag}" in
        i) input=${OPTARG};;
        n) name=${OPTARG};;
        o) output=${OPTARG};;
        m) models=${OPTARG};;
    esac
done

usage() {
    echo "-i <input audio file>"
    echo "-o <output dir>"
    echo "-m <model cache dir>"
    echo "-n <out file name>"
}

if [[ -z "$input" ]] || [[ -z "$output" ]] || [[ -z "$models" ]] || [[ -z "$name" ]]; then
    usage;
    exit;
fi

path="$(dirname -- "${BASH_SOURCE[0]}")"

docker run \
    --mount type=bind,source=$(realpath $input),target=/var/subtitles/$name \
    --mount type=bind,source=$(realpath $output),target=/var/subtitles/output \
    --mount type=bind,source=$(realpath $models),target=/var/subtitles/models \
    --mount type=bind,source=$(realpath $path/helper_script.sh),target=/var/subtitles/script.sh \
    --rm whisper_timestamped_cpu:latest \
    /var/subtitles/script.sh "$name"