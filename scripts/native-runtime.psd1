@{
    Files = @(
        # Validated CT2 4.7.1 Windows binary has WITH_CUDNN=OFF.
        'ctranslate2.dll', 'libiomp5md.dll',
        'cublas64_12.dll', 'cublasLt64_12.dll', 'onnxruntime.dll',
        'silero_vad_v6.onnx', 'msvcp140.dll', 'msvcp140_1.dll',
        'vcruntime140.dll', 'vcruntime140_1.dll'
    )
}
