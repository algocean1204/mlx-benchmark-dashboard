import 'dart:typed_data';

import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart' as frb;

class FrbAidashApi implements AidashApi {
  @override
  void init({required String rootPath}) {
    frb.init(rootPath: rootPath);
  }

  @override
  Future<frb.FrbDoctorReport> doctorReport() => frb.doctorReport();

  @override
  bool isBundleDeployMode() => frb.isBundleDeployMode();

  @override
  Stream<frb.FrbBootstrapEvent> envBootstrap() => frb.envBootstrap();

  @override
  List<frb.FrbOverviewRow> statsOverview({int? ctx}) =>
      frb.statsOverview(ctx: ctx == null ? null : toPlatformInt64(ctx));

  @override
  frb.FrbModelStats statsModel({required String id}) =>
      frb.statsModel(id: id);

  @override
  List<frb.FrbRunListRow> listRuns({String? model}) =>
      frb.listRuns(model: model);

  @override
  frb.FrbDeleteSummary deleteRun({required int id}) =>
      frb.deleteRun(id: toPlatformInt64(id));

  @override
  frb.FrbDeleteSummary deleteModel({required String id}) =>
      frb.deleteModel(id: id);

  @override
  List<frb.FrbCompareRow> compare({
    required List<String> models,
    int? ctx,
  }) =>
      frb.compare(
        models: models,
        ctx: ctx == null ? null : toPlatformInt64(ctx),
      );

  @override
  List<frb.FrbProfileRow> listProfiles() => frb.listProfiles();

  @override
  Future<int> benchStart({
    required String profileId,
    required int ctx,
    required frb.FrbBenchMode mode,
    String? prompt,
    String? imagePath,
    String? audioPath,
    String? benchTask,
    List<int>? sweepSteps,
  }) async {
    final id = await frb.benchStart(
      profileId: profileId,
      ctx: ctx,
      mode: mode,
      prompt: prompt,
      imagePath: imagePath,
      audioPath: audioPath,
      benchTask: benchTask,
      sweepSteps: sweepSteps == null
          ? null
          : Uint32List.fromList(sweepSteps),
    );
    return id.toInt();
  }

  @override
  void profileSetTask({
    required String profileId,
    required String task,
    required bool adjustBackend,
  }) {
    frb.profileSetTask(
      profileId: profileId,
      task: task,
      adjustBackend: adjustBackend,
    );
  }

  @override
  String profileTaskLabel({required String task}) =>
      frb.profileTaskLabel(task: task);

  @override
  Stream<frb.FrbBenchEvent> benchEvents() => frb.benchEvents();

  @override
  bool benchAbort() => frb.benchAbort();

  @override
  Future<void> serveStart({
    required String profileId,
    required int ctx,
  }) =>
      frb.serveStart(profileId: profileId, ctx: ctx);

  @override
  Future<void> serveStop() => frb.serveStop();

  @override
  bool chatShouldCompress({
    required int promptTokens,
    required int contextSize,
  }) =>
      frb.chatShouldCompress(
        promptTokens: promptTokens,
        contextSize: contextSize,
      );

  @override
  List<frb.FrbChatSessionRow> chatListSessions() => frb.chatListSessions();

  @override
  int chatCreateSession({required String profileId, required String title}) =>
      frb
          .chatCreateSession(profileId: profileId, title: title)
          .toInt();

  @override
  void chatDeleteSession({required int sessionId}) =>
      frb.chatDeleteSession(sessionId: toPlatformInt64(sessionId));

  @override
  List<frb.FrbChatMessageRow> chatLoadMessages({required int sessionId}) =>
      frb.chatLoadMessages(sessionId: toPlatformInt64(sessionId));

  @override
  int chatAppendMessage({
    required int sessionId,
    required String role,
    required String content,
    int? tokenCount,
  }) =>
      frb
          .chatAppendMessage(
            sessionId: toPlatformInt64(sessionId),
            role: role,
            content: content,
            tokenCount: tokenCount,
          )
          .toInt();

  @override
  void chatUpdateSessionTitle({
    required int sessionId,
    required String title,
  }) =>
      frb.chatUpdateSessionTitle(
        sessionId: toPlatformInt64(sessionId),
        title: title,
      );

  @override
  Future<String> chatSummarize({required List<frb.FrbChatMessage> messages}) =>
      frb.chatSummarize(messages: messages);

  @override
  Stream<frb.FrbChatStreamEvent> chatSend({
    required List<frb.FrbChatMessage> messages,
    String? imagePath,
  }) =>
      frb.chatSend(messages: messages, imagePath: imagePath);

  @override
  Future<frb.FrbAuthStatus> authStatus() => frb.authStatus();

  @override
  Future<String> authSet({required String token}) =>
      frb.authSet(token: token);

  @override
  Future<String> authImport() => frb.authImport();

  @override
  void authClear() => frb.authClear();

  @override
  Future<String> authVerifyToken({required String token}) =>
      frb.authVerifyToken(token: token);

  @override
  Stream<frb.FrbResourceSample> systemResources() => frb.systemResources();

  @override
  frb.FrbTierInfo tpsTier({required double decodeTps}) =>
      frb.tpsTier(decodeTps: decodeTps);

  @override
  String getProjectRoot() => frb.getProjectRoot();

  @override
  void setProjectRoot({required String path}) =>
      frb.setProjectRoot(path: path);

  @override
  Stream<frb.FrbFixProgress> runFixAction({required String command}) =>
      frb.runFixAction(command: command);

  @override
  String deviceLabel() => frb.deviceLabel();

  @override
  Future<frb.FrbCacheScanResult> cacheScan() => frb.cacheScan();

  @override
  Future<frb.FrbCacheDeleteResult> cacheDelete({required String repoId}) =>
      frb.cacheDelete(repoId: repoId);

  @override
  Future<frb.FrbDiskUsage> diskUsage() => frb.diskUsage();

  @override
  Future<List<frb.FrbHfSearchResult>> hfSearch({required String query}) =>
      frb.hfSearch(query: query);

  @override
  Future<int> hfModelSize({required String repoId}) async {
    final size = await frb.hfModelSize(repoId: repoId);
    return size.toInt();
  }

  @override
  Stream<frb.FrbDownloadProgress> hfDownloadStart({required String repoId}) =>
      frb.hfDownloadStart(repoId: repoId);

  @override
  bool hfDownloadCancel() => frb.hfDownloadCancel();

  @override
  String profileGenerate({required String repoId}) =>
      frb.profileGenerate(repoId: repoId);
}