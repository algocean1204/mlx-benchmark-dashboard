#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
// EXTRA BEGIN
typedef struct DartCObject *WireSyncRust2DartDco;
typedef struct WireSyncRust2DartSse {
  uint8_t *ptr;
  int32_t len;
} WireSyncRust2DartSse;

typedef int64_t DartPort;
typedef bool (*DartPostCObjectFnType)(DartPort port_id, void *message);
void store_dart_post_cobject(DartPostCObjectFnType ptr);
// EXTRA END
typedef struct _Dart_Handle* Dart_Handle;

typedef struct wire_cst_list_prim_u_8_strict {
  uint8_t *ptr;
  int32_t len;
} wire_cst_list_prim_u_8_strict;

typedef struct wire_cst_list_prim_u_32_strict {
  uint32_t *ptr;
  int32_t len;
} wire_cst_list_prim_u_32_strict;

typedef struct wire_cst_frb_chat_message {
  struct wire_cst_list_prim_u_8_strict *role;
  struct wire_cst_list_prim_u_8_strict *content;
} wire_cst_frb_chat_message;

typedef struct wire_cst_list_frb_chat_message {
  struct wire_cst_frb_chat_message *ptr;
  int32_t len;
} wire_cst_list_frb_chat_message;

typedef struct wire_cst_list_String {
  struct wire_cst_list_prim_u_8_strict **ptr;
  int32_t len;
} wire_cst_list_String;

typedef struct wire_cst_frb_tier_info {
  struct wire_cst_list_prim_u_8_strict *badge;
  struct wire_cst_list_prim_u_8_strict *label;
  struct wire_cst_list_prim_u_8_strict *key;
} wire_cst_frb_tier_info;

typedef struct wire_cst_frb_bench_result {
  int64_t run_id;
  struct wire_cst_list_prim_u_8_strict *status;
  uint32_t context_size;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  double *decode_tps;
  struct wire_cst_frb_tier_info *tier;
  double *ttft_ms;
  uint64_t peak_phys_footprint_bytes;
  uint64_t peak_mlx_active_bytes;
  struct wire_cst_list_prim_u_8_strict *error_message;
} wire_cst_frb_bench_result;

typedef struct wire_cst_frb_resource_sample {
  uint64_t ts;
  uint64_t phys_footprint_bytes;
  uint64_t *mlx_active_bytes;
  double cpu_pct;
  uint64_t sys_available_bytes;
  uint64_t total_memory_bytes;
  double *power_w;
  double *temp_c;
  bool *throttled;
} wire_cst_frb_resource_sample;

typedef struct wire_cst_frb_cache_repo_entry {
  struct wire_cst_list_prim_u_8_strict *repo_id;
  uint64_t size_bytes;
  struct wire_cst_list_prim_u_8_strict *last_modified;
  bool has_profile;
} wire_cst_frb_cache_repo_entry;

typedef struct wire_cst_list_frb_cache_repo_entry {
  struct wire_cst_frb_cache_repo_entry *ptr;
  int32_t len;
} wire_cst_list_frb_cache_repo_entry;

typedef struct wire_cst_frb_chat_message_row {
  int64_t id;
  int64_t session_id;
  struct wire_cst_list_prim_u_8_strict *role;
  struct wire_cst_list_prim_u_8_strict *content;
  struct wire_cst_list_prim_u_8_strict *created_at;
  int64_t *token_count;
} wire_cst_frb_chat_message_row;

typedef struct wire_cst_list_frb_chat_message_row {
  struct wire_cst_frb_chat_message_row *ptr;
  int32_t len;
} wire_cst_list_frb_chat_message_row;

typedef struct wire_cst_frb_chat_session_row {
  int64_t id;
  struct wire_cst_list_prim_u_8_strict *profile_id;
  struct wire_cst_list_prim_u_8_strict *title;
  struct wire_cst_list_prim_u_8_strict *created_at;
  struct wire_cst_list_prim_u_8_strict *updated_at;
} wire_cst_frb_chat_session_row;

typedef struct wire_cst_list_frb_chat_session_row {
  struct wire_cst_frb_chat_session_row *ptr;
  int32_t len;
} wire_cst_list_frb_chat_session_row;

typedef struct wire_cst_frb_compare_row {
  struct wire_cst_list_prim_u_8_strict *profile_id;
  struct wire_cst_list_prim_u_8_strict *display_name;
  struct wire_cst_list_prim_u_8_strict *model_type;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  int64_t context_requested;
  int64_t context_actual;
  bool context_substituted;
  double *decode_tps;
  struct wire_cst_frb_tier_info *tier;
  double *ttft_ms;
  int64_t *peak_phys_footprint_bytes;
  int64_t *peak_mlx_active_bytes;
  int64_t *tokens_in;
  int64_t *tokens_out;
  struct wire_cst_list_prim_u_8_strict *measured_at;
  struct wire_cst_list_prim_u_8_strict *hf_url;
  bool *use_draft;
} wire_cst_frb_compare_row;

typedef struct wire_cst_list_frb_compare_row {
  struct wire_cst_frb_compare_row *ptr;
  int32_t len;
} wire_cst_list_frb_compare_row;

typedef struct wire_cst_frb_context_stats_row {
  int64_t context_size;
  double decode_tps_min;
  double decode_tps_avg;
  double decode_tps_max;
  double ttft_avg_ms;
  int64_t run_count;
  int64_t peak_phys_footprint_bytes;
  int64_t peak_phys_avg_bytes;
} wire_cst_frb_context_stats_row;

typedef struct wire_cst_list_frb_context_stats_row {
  struct wire_cst_frb_context_stats_row *ptr;
  int32_t len;
} wire_cst_list_frb_context_stats_row;

typedef struct wire_cst_frb_doctor_item {
  struct wire_cst_list_prim_u_8_strict *category;
  struct wire_cst_list_prim_u_8_strict *name;
  struct wire_cst_list_prim_u_8_strict *status;
  struct wire_cst_list_prim_u_8_strict *detail;
  struct wire_cst_list_prim_u_8_strict *fix_action;
} wire_cst_frb_doctor_item;

typedef struct wire_cst_list_frb_doctor_item {
  struct wire_cst_frb_doctor_item *ptr;
  int32_t len;
} wire_cst_list_frb_doctor_item;

typedef struct wire_cst_frb_eval_template_item_result {
  struct wire_cst_list_prim_u_8_strict *template_id;
  struct wire_cst_list_prim_u_8_strict *description;
  uint32_t score;
  struct wire_cst_list_prim_u_8_strict *output_excerpt;
  uint64_t elapsed_ms;
} wire_cst_frb_eval_template_item_result;

typedef struct wire_cst_list_frb_eval_template_item_result {
  struct wire_cst_frb_eval_template_item_result *ptr;
  int32_t len;
} wire_cst_list_frb_eval_template_item_result;

typedef struct wire_cst_frb_eval_template_history_entry {
  uint32_t context_size;
  uint32_t total_score;
  struct wire_cst_list_prim_u_8_strict *created_at;
  struct wire_cst_list_frb_eval_template_item_result *items;
} wire_cst_frb_eval_template_history_entry;

typedef struct wire_cst_list_frb_eval_template_history_entry {
  struct wire_cst_frb_eval_template_history_entry *ptr;
  int32_t len;
} wire_cst_list_frb_eval_template_history_entry;

typedef struct wire_cst_frb_eval_template_info {
  struct wire_cst_list_prim_u_8_strict *id;
  uint32_t context_size;
  struct wire_cst_list_prim_u_8_strict *kind;
  struct wire_cst_list_prim_u_8_strict *description;
} wire_cst_frb_eval_template_info;

typedef struct wire_cst_list_frb_eval_template_info {
  struct wire_cst_frb_eval_template_info *ptr;
  int32_t len;
} wire_cst_list_frb_eval_template_info;

typedef struct wire_cst_frb_hf_search_result {
  struct wire_cst_list_prim_u_8_strict *repo_id;
  int64_t downloads;
  int64_t likes;
  struct wire_cst_list_prim_u_8_strict *pipeline_tag;
  bool installed;
} wire_cst_frb_hf_search_result;

typedef struct wire_cst_list_frb_hf_search_result {
  struct wire_cst_frb_hf_search_result *ptr;
  int32_t len;
} wire_cst_list_frb_hf_search_result;

typedef struct wire_cst_frb_context_pick {
  int64_t requested;
  int64_t actual;
  bool substituted;
} wire_cst_frb_context_pick;

typedef struct wire_cst_frb_overview_row {
  struct wire_cst_list_prim_u_8_strict *profile_id;
  struct wire_cst_list_prim_u_8_strict *display_name;
  struct wire_cst_list_prim_u_8_strict *model_type;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  double *decode_tps;
  struct wire_cst_frb_tier_info *tier;
  double *ttft_ms;
  struct wire_cst_frb_context_pick context;
  struct wire_cst_list_prim_u_8_strict *hf_url;
  struct wire_cst_list_prim_u_8_strict *measured_at;
} wire_cst_frb_overview_row;

typedef struct wire_cst_list_frb_overview_row {
  struct wire_cst_frb_overview_row *ptr;
  int32_t len;
} wire_cst_list_frb_overview_row;

typedef struct wire_cst_frb_profile_row {
  struct wire_cst_list_prim_u_8_strict *id;
  struct wire_cst_list_prim_u_8_strict *backend;
  struct wire_cst_list_prim_u_8_strict *model_type;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  uint32_t context_default;
  uint32_t context_min;
  uint32_t context_max;
  struct wire_cst_list_prim_u_32_strict *sweep_steps;
  struct wire_cst_list_prim_u_8_strict *filename;
  bool is_multimodal;
  struct wire_cst_list_prim_u_8_strict *draft_model;
  bool is_drafter;
} wire_cst_frb_profile_row;

typedef struct wire_cst_list_frb_profile_row {
  struct wire_cst_frb_profile_row *ptr;
  int32_t len;
} wire_cst_list_frb_profile_row;

typedef struct wire_cst_frb_run_list_row {
  int64_t run_id;
  struct wire_cst_list_prim_u_8_strict *profile_id;
  struct wire_cst_list_prim_u_8_strict *display_name;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  struct wire_cst_list_prim_u_8_strict *kind;
  int64_t *context_size;
  struct wire_cst_list_prim_u_8_strict *status;
  double *decode_tps;
  int64_t *peak_phys_footprint_bytes;
  struct wire_cst_frb_tier_info *tier;
  struct wire_cst_list_prim_u_8_strict *ended_at;
  bool *use_draft;
} wire_cst_frb_run_list_row;

typedef struct wire_cst_list_frb_run_list_row {
  struct wire_cst_frb_run_list_row *ptr;
  int32_t len;
} wire_cst_list_frb_run_list_row;

typedef struct wire_cst_frb_token_source_status {
  struct wire_cst_list_prim_u_8_strict *source;
  struct wire_cst_list_prim_u_8_strict *label;
  bool present;
} wire_cst_frb_token_source_status;

typedef struct wire_cst_list_frb_token_source_status {
  struct wire_cst_frb_token_source_status *ptr;
  int32_t len;
} wire_cst_list_frb_token_source_status;

typedef struct wire_cst_frb_auth_status {
  struct wire_cst_list_frb_token_source_status *sources;
  struct wire_cst_list_prim_u_8_strict *active_source;
  struct wire_cst_list_prim_u_8_strict *masked_token;
  struct wire_cst_list_prim_u_8_strict *whoami_user;
  bool can_import;
} wire_cst_frb_auth_status;

typedef struct wire_cst_FrbBenchEvent_StateChanged {
  struct wire_cst_list_prim_u_8_strict *from;
  struct wire_cst_list_prim_u_8_strict *to;
} wire_cst_FrbBenchEvent_StateChanged;

typedef struct wire_cst_FrbBenchEvent_Sample {
  struct wire_cst_frb_resource_sample *field0;
} wire_cst_FrbBenchEvent_Sample;

typedef struct wire_cst_FrbBenchEvent_Token {
  uint32_t index;
  struct wire_cst_list_prim_u_8_strict *text;
} wire_cst_FrbBenchEvent_Token;

typedef struct wire_cst_FrbBenchEvent_RunFinished {
  uint64_t run_id;
  struct wire_cst_list_prim_u_8_strict *status;
  struct wire_cst_frb_bench_result *result;
} wire_cst_FrbBenchEvent_RunFinished;

typedef struct wire_cst_FrbBenchEvent_Log {
  struct wire_cst_list_prim_u_8_strict *level;
  struct wire_cst_list_prim_u_8_strict *message;
} wire_cst_FrbBenchEvent_Log;

typedef struct wire_cst_FrbBenchEvent_Progress {
  struct wire_cst_list_prim_u_8_strict *message;
} wire_cst_FrbBenchEvent_Progress;

typedef union FrbBenchEventKind {
  struct wire_cst_FrbBenchEvent_StateChanged StateChanged;
  struct wire_cst_FrbBenchEvent_Sample Sample;
  struct wire_cst_FrbBenchEvent_Token Token;
  struct wire_cst_FrbBenchEvent_RunFinished RunFinished;
  struct wire_cst_FrbBenchEvent_Log Log;
  struct wire_cst_FrbBenchEvent_Progress Progress;
} FrbBenchEventKind;

typedef struct wire_cst_frb_bench_event {
  int32_t tag;
  union FrbBenchEventKind kind;
} wire_cst_frb_bench_event;

typedef struct wire_cst_frb_bootstrap_event {
  struct wire_cst_list_prim_u_8_strict *step;
  struct wire_cst_list_prim_u_8_strict *kind;
  struct wire_cst_list_prim_u_8_strict *message;
  bool *success;
} wire_cst_frb_bootstrap_event;

typedef struct wire_cst_frb_cache_delete_result {
  struct wire_cst_list_prim_u_8_strict *repo_id;
  bool deleted;
  uint64_t freed_bytes;
  struct wire_cst_list_prim_u_8_strict *error;
} wire_cst_frb_cache_delete_result;

typedef struct wire_cst_frb_cache_scan_result {
  struct wire_cst_list_prim_u_8_strict *cache_dir;
  uint64_t total_size_bytes;
  uintptr_t repo_count;
  struct wire_cst_list_frb_cache_repo_entry *repos;
} wire_cst_frb_cache_scan_result;

typedef struct wire_cst_frb_chat_stream_event {
  bool is_done;
  struct wire_cst_list_prim_u_8_strict *text;
  uint32_t prompt_tokens;
  uint32_t completion_tokens;
} wire_cst_frb_chat_stream_event;

typedef struct wire_cst_frb_delete_summary {
  int64_t runs;
  int64_t samples;
  int64_t results;
} wire_cst_frb_delete_summary;

typedef struct wire_cst_frb_disk_usage {
  uint64_t total_bytes;
  uint64_t available_bytes;
  struct wire_cst_list_prim_u_8_strict *cache_dir;
  uint64_t cache_total_bytes;
} wire_cst_frb_disk_usage;

typedef struct wire_cst_frb_doctor_report {
  struct wire_cst_list_frb_doctor_item *items;
} wire_cst_frb_doctor_report;

typedef struct wire_cst_frb_download_progress {
  struct wire_cst_list_prim_u_8_strict *line;
  double *percent;
  bool done;
  bool success;
} wire_cst_frb_download_progress;

typedef struct wire_cst_FrbEvalTemplateEvent_Started {
  struct wire_cst_list_prim_u_8_strict *template_id;
  uint32_t index;
  uint32_t total;
} wire_cst_FrbEvalTemplateEvent_Started;

typedef struct wire_cst_FrbEvalTemplateEvent_Completed {
  struct wire_cst_list_prim_u_8_strict *template_id;
  uint32_t score;
  uint64_t elapsed_ms;
} wire_cst_FrbEvalTemplateEvent_Completed;

typedef struct wire_cst_FrbEvalTemplateEvent_Finished {
  uint32_t total_score;
  struct wire_cst_list_frb_eval_template_item_result *items;
} wire_cst_FrbEvalTemplateEvent_Finished;

typedef struct wire_cst_FrbEvalTemplateEvent_Log {
  struct wire_cst_list_prim_u_8_strict *message;
} wire_cst_FrbEvalTemplateEvent_Log;

typedef union FrbEvalTemplateEventKind {
  struct wire_cst_FrbEvalTemplateEvent_Started Started;
  struct wire_cst_FrbEvalTemplateEvent_Completed Completed;
  struct wire_cst_FrbEvalTemplateEvent_Finished Finished;
  struct wire_cst_FrbEvalTemplateEvent_Log Log;
} FrbEvalTemplateEventKind;

typedef struct wire_cst_frb_eval_template_event {
  int32_t tag;
  union FrbEvalTemplateEventKind kind;
} wire_cst_frb_eval_template_event;

typedef struct wire_cst_frb_fix_progress {
  struct wire_cst_list_prim_u_8_strict *line;
  bool done;
  bool success;
  int32_t *exit_code;
} wire_cst_frb_fix_progress;

typedef struct wire_cst_frb_model_stats {
  struct wire_cst_list_prim_u_8_strict *profile_id;
  struct wire_cst_list_prim_u_8_strict *display_name;
  struct wire_cst_list_prim_u_8_strict *generation_kind;
  int64_t total_runs;
  struct wire_cst_list_prim_u_8_strict *latest_measured_at;
  struct wire_cst_frb_tier_info *current_tier;
  double *current_decode_tps;
  int64_t peak_phys_footprint_bytes;
  int64_t peak_mlx_active_bytes;
  struct wire_cst_list_prim_u_8_strict *hf_url;
  struct wire_cst_list_frb_context_stats_row *by_context;
} wire_cst_frb_model_stats;

typedef struct wire_cst_record_u_64_u_64_u_64_f_64 {
  uint64_t field0;
  uint64_t field1;
  uint64_t field2;
  double field3;
} wire_cst_record_u_64_u_64_u_64_f_64;

WireSyncRust2DartDco frbgen_app_wire__crate__api__auth_clear(void);

void frbgen_app_wire__crate__api__auth_import(int64_t port_);

void frbgen_app_wire__crate__api__auth_set(int64_t port_,
                                           struct wire_cst_list_prim_u_8_strict *token);

void frbgen_app_wire__crate__api__auth_status(int64_t port_);

void frbgen_app_wire__crate__api__auth_verify_token(int64_t port_,
                                                    struct wire_cst_list_prim_u_8_strict *token);

WireSyncRust2DartDco frbgen_app_wire__crate__api__bench_abort(void);

void frbgen_app_wire__crate__api__bench_events(int64_t port_,
                                               struct wire_cst_list_prim_u_8_strict *sink);

void frbgen_app_wire__crate__api__bench_start(int64_t port_,
                                              struct wire_cst_list_prim_u_8_strict *profile_id,
                                              uint32_t ctx,
                                              int32_t mode,
                                              struct wire_cst_list_prim_u_8_strict *prompt,
                                              struct wire_cst_list_prim_u_8_strict *image_path,
                                              struct wire_cst_list_prim_u_8_strict *audio_path,
                                              struct wire_cst_list_prim_u_8_strict *bench_task,
                                              struct wire_cst_list_prim_u_32_strict *sweep_steps,
                                              bool *use_draft);

void frbgen_app_wire__crate__api__cache_delete(int64_t port_,
                                               struct wire_cst_list_prim_u_8_strict *repo_id);

void frbgen_app_wire__crate__api__cache_scan(int64_t port_);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_append_message(int64_t session_id,
                                                                      struct wire_cst_list_prim_u_8_strict *role,
                                                                      struct wire_cst_list_prim_u_8_strict *content,
                                                                      uint32_t *token_count);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_create_session(struct wire_cst_list_prim_u_8_strict *profile_id,
                                                                      struct wire_cst_list_prim_u_8_strict *title);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_delete_session(int64_t session_id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_list_sessions(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_load_messages(int64_t session_id);

void frbgen_app_wire__crate__api__chat_send(int64_t port_,
                                            struct wire_cst_list_frb_chat_message *messages,
                                            struct wire_cst_list_prim_u_8_strict *image_path,
                                            struct wire_cst_list_prim_u_8_strict *sink);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_should_compress(uint32_t prompt_tokens,
                                                                       uint32_t context_size);

void frbgen_app_wire__crate__api__chat_summarize(int64_t port_,
                                                 struct wire_cst_list_frb_chat_message *messages);

WireSyncRust2DartDco frbgen_app_wire__crate__api__chat_update_session_title(int64_t session_id,
                                                                            struct wire_cst_list_prim_u_8_strict *title);

WireSyncRust2DartDco frbgen_app_wire__crate__api__compare(struct wire_cst_list_String *models,
                                                          int64_t *ctx);

WireSyncRust2DartDco frbgen_app_wire__crate__api__delete_model(struct wire_cst_list_prim_u_8_strict *id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__delete_run(int64_t id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__device_label(void);

void frbgen_app_wire__crate__api__disk_usage(int64_t port_);

void frbgen_app_wire__crate__api__doctor_report(int64_t port_);

void frbgen_app_wire__crate__api__env_bootstrap(int64_t port_,
                                                struct wire_cst_list_prim_u_8_strict *sink);

WireSyncRust2DartDco frbgen_app_wire__crate__api__eval_template_history(struct wire_cst_list_prim_u_8_strict *profile_id,
                                                                        uint32_t *context_size);

WireSyncRust2DartDco frbgen_app_wire__crate__api__eval_template_list(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__eval_template_measurable_contexts(struct wire_cst_list_prim_u_8_strict *profile_id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__eval_template_preview_prompt(struct wire_cst_list_prim_u_8_strict *template_id);

void frbgen_app_wire__crate__api__eval_template_run(int64_t port_,
                                                    struct wire_cst_list_prim_u_8_strict *profile_id,
                                                    uint32_t context_size,
                                                    struct wire_cst_list_prim_u_8_strict *sink);

WireSyncRust2DartDco frbgen_app_wire__crate__api__get_project_root(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__hf_download_cancel(void);

void frbgen_app_wire__crate__api__hf_download_start(int64_t port_,
                                                    struct wire_cst_list_prim_u_8_strict *repo_id,
                                                    struct wire_cst_list_prim_u_8_strict *sink);

void frbgen_app_wire__crate__api__hf_model_size(int64_t port_,
                                                struct wire_cst_list_prim_u_8_strict *repo_id);

void frbgen_app_wire__crate__api__hf_search(int64_t port_,
                                            struct wire_cst_list_prim_u_8_strict *query);

WireSyncRust2DartDco frbgen_app_wire__crate__api__init(struct wire_cst_list_prim_u_8_strict *root_path);

WireSyncRust2DartDco frbgen_app_wire__crate__api__is_bundle_deploy_mode(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__list_drafter_profiles(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__list_profiles(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__list_runs(struct wire_cst_list_prim_u_8_strict *model);

WireSyncRust2DartDco frbgen_app_wire__crate__api__measured_contexts(void);

WireSyncRust2DartDco frbgen_app_wire__crate__api__profile_generate(struct wire_cst_list_prim_u_8_strict *repo_id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__profile_set_draft_model(struct wire_cst_list_prim_u_8_strict *profile_id,
                                                                          struct wire_cst_list_prim_u_8_strict *draft_model);

WireSyncRust2DartDco frbgen_app_wire__crate__api__profile_set_task(struct wire_cst_list_prim_u_8_strict *profile_id,
                                                                   struct wire_cst_list_prim_u_8_strict *task,
                                                                   bool adjust_backend);

WireSyncRust2DartDco frbgen_app_wire__crate__api__profile_task_label(struct wire_cst_list_prim_u_8_strict *task);

void frbgen_app_wire__crate__api__run_fix_action(int64_t port_,
                                                 struct wire_cst_list_prim_u_8_strict *command,
                                                 struct wire_cst_list_prim_u_8_strict *sink);

void frbgen_app_wire__crate__api__serve_start(int64_t port_,
                                              struct wire_cst_list_prim_u_8_strict *profile_id,
                                              uint32_t ctx);

void frbgen_app_wire__crate__api__serve_stop(int64_t port_);

WireSyncRust2DartDco frbgen_app_wire__crate__api__set_project_root(struct wire_cst_list_prim_u_8_strict *path);

WireSyncRust2DartDco frbgen_app_wire__crate__api__stats_model(struct wire_cst_list_prim_u_8_strict *id);

WireSyncRust2DartDco frbgen_app_wire__crate__api__stats_overview(int64_t *ctx);

WireSyncRust2DartDco frbgen_app_wire__crate__api__system_memory_info(void);

void frbgen_app_wire__crate__api__system_resources(int64_t port_,
                                                   struct wire_cst_list_prim_u_8_strict *sink);

WireSyncRust2DartDco frbgen_app_wire__crate__api__tps_tier(double decode_tps);

bool *frbgen_app_cst_new_box_autoadd_bool(bool value);

double *frbgen_app_cst_new_box_autoadd_f_64(double value);

struct wire_cst_frb_bench_result *frbgen_app_cst_new_box_autoadd_frb_bench_result(void);

struct wire_cst_frb_resource_sample *frbgen_app_cst_new_box_autoadd_frb_resource_sample(void);

struct wire_cst_frb_tier_info *frbgen_app_cst_new_box_autoadd_frb_tier_info(void);

int32_t *frbgen_app_cst_new_box_autoadd_i_32(int32_t value);

int64_t *frbgen_app_cst_new_box_autoadd_i_64(int64_t value);

uint32_t *frbgen_app_cst_new_box_autoadd_u_32(uint32_t value);

uint64_t *frbgen_app_cst_new_box_autoadd_u_64(uint64_t value);

struct wire_cst_list_String *frbgen_app_cst_new_list_String(int32_t len);

struct wire_cst_list_frb_cache_repo_entry *frbgen_app_cst_new_list_frb_cache_repo_entry(int32_t len);

struct wire_cst_list_frb_chat_message *frbgen_app_cst_new_list_frb_chat_message(int32_t len);

struct wire_cst_list_frb_chat_message_row *frbgen_app_cst_new_list_frb_chat_message_row(int32_t len);

struct wire_cst_list_frb_chat_session_row *frbgen_app_cst_new_list_frb_chat_session_row(int32_t len);

struct wire_cst_list_frb_compare_row *frbgen_app_cst_new_list_frb_compare_row(int32_t len);

struct wire_cst_list_frb_context_stats_row *frbgen_app_cst_new_list_frb_context_stats_row(int32_t len);

struct wire_cst_list_frb_doctor_item *frbgen_app_cst_new_list_frb_doctor_item(int32_t len);

struct wire_cst_list_frb_eval_template_history_entry *frbgen_app_cst_new_list_frb_eval_template_history_entry(int32_t len);

struct wire_cst_list_frb_eval_template_info *frbgen_app_cst_new_list_frb_eval_template_info(int32_t len);

struct wire_cst_list_frb_eval_template_item_result *frbgen_app_cst_new_list_frb_eval_template_item_result(int32_t len);

struct wire_cst_list_frb_hf_search_result *frbgen_app_cst_new_list_frb_hf_search_result(int32_t len);

struct wire_cst_list_frb_overview_row *frbgen_app_cst_new_list_frb_overview_row(int32_t len);

struct wire_cst_list_frb_profile_row *frbgen_app_cst_new_list_frb_profile_row(int32_t len);

struct wire_cst_list_frb_run_list_row *frbgen_app_cst_new_list_frb_run_list_row(int32_t len);

struct wire_cst_list_frb_token_source_status *frbgen_app_cst_new_list_frb_token_source_status(int32_t len);

struct wire_cst_list_prim_u_32_strict *frbgen_app_cst_new_list_prim_u_32_strict(int32_t len);

struct wire_cst_list_prim_u_8_strict *frbgen_app_cst_new_list_prim_u_8_strict(int32_t len);
static int64_t dummy_method_to_enforce_bundling(void) {
    int64_t dummy_var = 0;
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_bool);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_f_64);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_frb_bench_result);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_frb_resource_sample);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_frb_tier_info);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_i_32);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_i_64);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_u_32);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_box_autoadd_u_64);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_String);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_cache_repo_entry);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_chat_message);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_chat_message_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_chat_session_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_compare_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_context_stats_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_doctor_item);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_eval_template_history_entry);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_eval_template_info);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_eval_template_item_result);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_hf_search_result);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_overview_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_profile_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_run_list_row);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_frb_token_source_status);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_prim_u_32_strict);
    dummy_var ^= ((int64_t) (void*) frbgen_app_cst_new_list_prim_u_8_strict);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__auth_clear);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__auth_import);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__auth_set);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__auth_status);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__auth_verify_token);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__bench_abort);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__bench_events);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__bench_start);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__cache_delete);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__cache_scan);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_append_message);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_create_session);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_delete_session);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_list_sessions);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_load_messages);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_send);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_should_compress);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_summarize);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__chat_update_session_title);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__compare);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__delete_model);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__delete_run);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__device_label);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__disk_usage);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__doctor_report);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__env_bootstrap);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__eval_template_history);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__eval_template_list);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__eval_template_measurable_contexts);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__eval_template_preview_prompt);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__eval_template_run);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__get_project_root);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__hf_download_cancel);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__hf_download_start);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__hf_model_size);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__hf_search);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__init);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__is_bundle_deploy_mode);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__list_drafter_profiles);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__list_profiles);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__list_runs);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__measured_contexts);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__profile_generate);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__profile_set_draft_model);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__profile_set_task);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__profile_task_label);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__run_fix_action);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__serve_start);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__serve_stop);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__set_project_root);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__stats_model);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__stats_overview);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__system_memory_info);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__system_resources);
    dummy_var ^= ((int64_t) (void*) frbgen_app_wire__crate__api__tps_tier);
    dummy_var ^= ((int64_t) (void*) store_dart_post_cobject);
    return dummy_var;
}
