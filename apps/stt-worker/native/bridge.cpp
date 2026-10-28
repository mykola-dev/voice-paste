// CT2 4.7.1 ABI adapter. No Python. All native owners/exceptions stay here.
#include <ctranslate2/models/whisper.h>
#include <ctranslate2/logging.h>
#include <cstdio>
#include <cmath>
#include <memory>
#include <stdexcept>
#include <algorithm>
#include <cstdint>

using namespace ctranslate2;
struct Engine { std::unique_ptr<models::Whisper> model; int device; };
struct Encoded { StorageView features; Engine* engine; };
namespace {
int fail(char* error, size_t capacity, const char* message) noexcept {
    if (error && capacity) std::snprintf(error, capacity, "%s", message);
    return 1;
}
std::unique_ptr<models::Whisper> load(const char* path, bool gpu) {
    ReplicaPoolConfig config;
    config.num_threads_per_replica = 2;
    config.max_queued_batches = 1;
    return std::make_unique<models::Whisper>(path, gpu ? Device::CUDA : Device::CPU,
        gpu ? ComputeType::FLOAT16 : ComputeType::INT8, std::vector<int>{0}, false, config);
}
}

extern "C" int vp_open(const char* path, int device, void** result, int* actual,
                        size_t* mels, char* error, size_t capacity) noexcept {
    try {
        if (!path || !result || !actual || !mels || device < 0 || device > 2) throw std::runtime_error("Invalid engine parameters");
        set_log_level(LogLevel::Warning);
        auto engine = std::make_unique<Engine>();
        engine->device = device == 0 ? 0 : 1;
        try { engine->model = load(path, device != 0); }
        catch (const std::exception&) {
            if (device != 2) throw;
            engine->model = load(path, false); engine->device = 0;
        }
        *mels = engine->model->n_mels(); *actual = engine->device;
        *result = engine.release(); return 0;
    } catch (const std::exception& e) { return fail(error, capacity, e.what()); }
      catch (...) { return fail(error, capacity, "Unknown CT2 load error"); }
}
extern "C" void vp_close(void* engine) noexcept { delete static_cast<Engine*>(engine); }

extern "C" int vp_encode(void* pointer, const float* values, size_t count,
                          void** result, char* error, size_t capacity) noexcept {
    try {
        auto engine = static_cast<Engine*>(pointer);
        if (!engine || !values || !result || count != engine->model->n_mels() * 3000) throw std::runtime_error("Invalid feature layout");
        for (size_t i = 0; i < count; ++i) if (!std::isfinite(values[i])) throw std::runtime_error("Nonfinite feature");
        StorageView features({1, static_cast<dim_t>(engine->model->n_mels()), 3000}, std::vector<float>(values, values + count), Device::CPU);
        auto encoded = std::make_unique<Encoded>(Encoded{engine->model->encode(features, false).get(), engine});
        *result = encoded.release(); return 0;
    } catch (const std::exception& e) { return fail(error, capacity, e.what()); }
      catch (...) { return fail(error, capacity, "Unknown CT2 encoder error"); }
}
extern "C" void vp_encoded_close(void* encoded) noexcept { delete static_cast<Encoded*>(encoded); }

extern "C" int vp_language(void* pointer, char* language, size_t language_capacity,
                            char* error, size_t capacity) noexcept {
    try {
        auto encoded = static_cast<Encoded*>(pointer);
        if (!encoded || !language || !language_capacity) throw std::runtime_error("Invalid language parameters");
        auto futures = encoded->engine->model->detect_language(encoded->features);
        auto languages = futures.at(0).get();
        if (languages.empty()) throw std::runtime_error("Empty language detection");
        if (languages.front().first.size() >= language_capacity) throw std::runtime_error("Language buffer too small");
        std::snprintf(language, language_capacity, "%s", languages.front().first.c_str()); return 0;
    } catch (const std::exception& e) { return fail(error, capacity, e.what()); }
      catch (...) { return fail(error, capacity, "Unknown language detection error"); }
}

extern "C" int vp_generate(void* pointer, const uint32_t* prompt, size_t prompt_count,
                            const int32_t* suppressed, size_t suppressed_count,
                            size_t beam, float temperature, uint32_t* output, size_t output_capacity,
                            size_t* output_count, float* score, float* no_speech,
                            char* error, size_t capacity) noexcept {
    try {
        auto encoded = static_cast<Encoded*>(pointer);
        if (!encoded || !prompt || !prompt_count || prompt_count >= 448 || !output || output_capacity < 448
            || !output_count || !score || !no_speech || beam < 1 || beam > 10)
            throw std::runtime_error("Invalid generation parameters");
        models::WhisperOptions options;
        options.beam_size = temperature > 0 ? 1 : beam;
        options.num_hypotheses = temperature > 0 ? 5 : 1;
        options.sampling_topk = temperature > 0 ? 0 : 1;
        options.sampling_temperature = temperature > 0 ? temperature : 1;
        options.max_length = 448;
        options.return_scores = true;
        options.return_no_speech_prob = true;
        options.suppress_tokens.assign(suppressed, suppressed + suppressed_count);
        std::vector<std::vector<size_t>> prompts{std::vector<size_t>(prompt, prompt + prompt_count)};
        auto futures = encoded->engine->model->generate(encoded->features, std::move(prompts), options);
        auto result = futures.at(0).get();
        const auto& ids = result.sequences_ids.at(0);
        if (ids.size() > output_capacity || !std::isfinite(result.scores.at(0)) || !std::isfinite(result.no_speech_prob))
            throw std::runtime_error("Invalid native generation result");
        for (size_t i = 0; i < ids.size(); ++i) output[i] = static_cast<uint32_t>(ids[i]);
        *output_count = ids.size(); *score = result.scores.at(0); *no_speech = result.no_speech_prob;
        return 0;
    } catch (const std::exception& e) { return fail(error, capacity, e.what()); }
      catch (...) { return fail(error, capacity, "Unknown CT2 decoder error"); }
}
