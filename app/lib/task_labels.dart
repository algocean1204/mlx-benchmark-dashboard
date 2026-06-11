/// 태스크 한국어 라벨·뱃지 — UI 단일 소스.
class TaskLabels {
  TaskLabels._();

  static const allTasks = [
    'llm',
    'multimodal',
    'asr',
    'tts',
    'image_gen',
    'video_gen',
  ];

  static const _labels = <String, String>{
    'llm': '텍스트 생성',
    'multimodal': '멀티모달(이미지+텍스트)',
    'asr': '음성→텍스트(STT)',
    'tts': '텍스트→음성(TTS)',
    'image_gen': '이미지 생성',
    'video_gen': '동영상 생성',
  };

  static const _badges = <String, String>{
    'asr': 'STT',
    'tts': 'TTS',
    'image_gen': '이미지',
    'multimodal': '멀티모달',
    'video_gen': '동영상',
  };

  static const _pipelineTagMap = <String, String>{
    'text-generation': '텍스트 생성',
    'text-to-speech': '텍스트→음성(TTS)',
    'automatic-speech-recognition': '음성→텍스트(STT)',
    'image-to-text': '멀티모달(이미지+텍스트)',
    'image-text-to-text': '멀티모달(이미지+텍스트)',
    'visual-question-answering': '멀티모달(이미지+텍스트)',
    'text-to-image': '이미지 생성',
    'text-to-video': '동영상 생성',
  };

  static String label(String task) => _labels[task] ?? task;

  static String? badge(String task) => _badges[task];

  static bool showBadge(String task) => task != 'llm' && badge(task) != null;

  static bool isChatCapable(String task) =>
      task == 'llm' || task == 'multimodal';

  static bool isBenchOnly(String task) =>
      task == 'asr' || task == 'tts' || task == 'image_gen' || task == 'video_gen';

  static bool isVideoUnsupported(String task) => task == 'video_gen';

  /// 벤치 탭에서 선택 가능한 태스크 목록.
  static List<String> benchTasksForProfile(String modelType) {
    if (modelType == 'multimodal') return ['llm', 'multimodal'];
    return [modelType];
  }

  static bool usesContext(String task) => task == 'llm' || task == 'multimodal';

  static bool usesTpsTier(String task) => task == 'llm' || task == 'multimodal';

  static String pipelineTagLabel(String? tag) {
    if (tag == null || tag.isEmpty) return '';
    return _pipelineTagMap[tag] ?? tag;
  }
}