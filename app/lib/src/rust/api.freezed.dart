// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$FrbBenchEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'FrbBenchEvent()';
}


}

/// @nodoc
class $FrbBenchEventCopyWith<$Res>  {
$FrbBenchEventCopyWith(FrbBenchEvent _, $Res Function(FrbBenchEvent) __);
}


/// Adds pattern-matching-related methods to [FrbBenchEvent].
extension FrbBenchEventPatterns on FrbBenchEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( FrbBenchEvent_StateChanged value)?  stateChanged,TResult Function( FrbBenchEvent_Sample value)?  sample,TResult Function( FrbBenchEvent_Token value)?  token,TResult Function( FrbBenchEvent_WatchdogWarn value)?  watchdogWarn,TResult Function( FrbBenchEvent_WatchdogKill value)?  watchdogKill,TResult Function( FrbBenchEvent_RunFinished value)?  runFinished,TResult Function( FrbBenchEvent_Log value)?  log,TResult Function( FrbBenchEvent_Progress value)?  progress,required TResult orElse(),}){
final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged() when stateChanged != null:
return stateChanged(_that);case FrbBenchEvent_Sample() when sample != null:
return sample(_that);case FrbBenchEvent_Token() when token != null:
return token(_that);case FrbBenchEvent_WatchdogWarn() when watchdogWarn != null:
return watchdogWarn(_that);case FrbBenchEvent_WatchdogKill() when watchdogKill != null:
return watchdogKill(_that);case FrbBenchEvent_RunFinished() when runFinished != null:
return runFinished(_that);case FrbBenchEvent_Log() when log != null:
return log(_that);case FrbBenchEvent_Progress() when progress != null:
return progress(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( FrbBenchEvent_StateChanged value)  stateChanged,required TResult Function( FrbBenchEvent_Sample value)  sample,required TResult Function( FrbBenchEvent_Token value)  token,required TResult Function( FrbBenchEvent_WatchdogWarn value)  watchdogWarn,required TResult Function( FrbBenchEvent_WatchdogKill value)  watchdogKill,required TResult Function( FrbBenchEvent_RunFinished value)  runFinished,required TResult Function( FrbBenchEvent_Log value)  log,required TResult Function( FrbBenchEvent_Progress value)  progress,}){
final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged():
return stateChanged(_that);case FrbBenchEvent_Sample():
return sample(_that);case FrbBenchEvent_Token():
return token(_that);case FrbBenchEvent_WatchdogWarn():
return watchdogWarn(_that);case FrbBenchEvent_WatchdogKill():
return watchdogKill(_that);case FrbBenchEvent_RunFinished():
return runFinished(_that);case FrbBenchEvent_Log():
return log(_that);case FrbBenchEvent_Progress():
return progress(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( FrbBenchEvent_StateChanged value)?  stateChanged,TResult? Function( FrbBenchEvent_Sample value)?  sample,TResult? Function( FrbBenchEvent_Token value)?  token,TResult? Function( FrbBenchEvent_WatchdogWarn value)?  watchdogWarn,TResult? Function( FrbBenchEvent_WatchdogKill value)?  watchdogKill,TResult? Function( FrbBenchEvent_RunFinished value)?  runFinished,TResult? Function( FrbBenchEvent_Log value)?  log,TResult? Function( FrbBenchEvent_Progress value)?  progress,}){
final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged() when stateChanged != null:
return stateChanged(_that);case FrbBenchEvent_Sample() when sample != null:
return sample(_that);case FrbBenchEvent_Token() when token != null:
return token(_that);case FrbBenchEvent_WatchdogWarn() when watchdogWarn != null:
return watchdogWarn(_that);case FrbBenchEvent_WatchdogKill() when watchdogKill != null:
return watchdogKill(_that);case FrbBenchEvent_RunFinished() when runFinished != null:
return runFinished(_that);case FrbBenchEvent_Log() when log != null:
return log(_that);case FrbBenchEvent_Progress() when progress != null:
return progress(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String from,  String to)?  stateChanged,TResult Function( FrbResourceSample field0)?  sample,TResult Function( int index,  String text)?  token,TResult Function()?  watchdogWarn,TResult Function()?  watchdogKill,TResult Function( BigInt runId,  String status,  FrbBenchResult? result)?  runFinished,TResult Function( String level,  String message)?  log,TResult Function( String message)?  progress,required TResult orElse(),}) {final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged() when stateChanged != null:
return stateChanged(_that.from,_that.to);case FrbBenchEvent_Sample() when sample != null:
return sample(_that.field0);case FrbBenchEvent_Token() when token != null:
return token(_that.index,_that.text);case FrbBenchEvent_WatchdogWarn() when watchdogWarn != null:
return watchdogWarn();case FrbBenchEvent_WatchdogKill() when watchdogKill != null:
return watchdogKill();case FrbBenchEvent_RunFinished() when runFinished != null:
return runFinished(_that.runId,_that.status,_that.result);case FrbBenchEvent_Log() when log != null:
return log(_that.level,_that.message);case FrbBenchEvent_Progress() when progress != null:
return progress(_that.message);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String from,  String to)  stateChanged,required TResult Function( FrbResourceSample field0)  sample,required TResult Function( int index,  String text)  token,required TResult Function()  watchdogWarn,required TResult Function()  watchdogKill,required TResult Function( BigInt runId,  String status,  FrbBenchResult? result)  runFinished,required TResult Function( String level,  String message)  log,required TResult Function( String message)  progress,}) {final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged():
return stateChanged(_that.from,_that.to);case FrbBenchEvent_Sample():
return sample(_that.field0);case FrbBenchEvent_Token():
return token(_that.index,_that.text);case FrbBenchEvent_WatchdogWarn():
return watchdogWarn();case FrbBenchEvent_WatchdogKill():
return watchdogKill();case FrbBenchEvent_RunFinished():
return runFinished(_that.runId,_that.status,_that.result);case FrbBenchEvent_Log():
return log(_that.level,_that.message);case FrbBenchEvent_Progress():
return progress(_that.message);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String from,  String to)?  stateChanged,TResult? Function( FrbResourceSample field0)?  sample,TResult? Function( int index,  String text)?  token,TResult? Function()?  watchdogWarn,TResult? Function()?  watchdogKill,TResult? Function( BigInt runId,  String status,  FrbBenchResult? result)?  runFinished,TResult? Function( String level,  String message)?  log,TResult? Function( String message)?  progress,}) {final _that = this;
switch (_that) {
case FrbBenchEvent_StateChanged() when stateChanged != null:
return stateChanged(_that.from,_that.to);case FrbBenchEvent_Sample() when sample != null:
return sample(_that.field0);case FrbBenchEvent_Token() when token != null:
return token(_that.index,_that.text);case FrbBenchEvent_WatchdogWarn() when watchdogWarn != null:
return watchdogWarn();case FrbBenchEvent_WatchdogKill() when watchdogKill != null:
return watchdogKill();case FrbBenchEvent_RunFinished() when runFinished != null:
return runFinished(_that.runId,_that.status,_that.result);case FrbBenchEvent_Log() when log != null:
return log(_that.level,_that.message);case FrbBenchEvent_Progress() when progress != null:
return progress(_that.message);case _:
  return null;

}
}

}

/// @nodoc


class FrbBenchEvent_StateChanged extends FrbBenchEvent {
  const FrbBenchEvent_StateChanged({required this.from, required this.to}): super._();
  

 final  String from;
 final  String to;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_StateChangedCopyWith<FrbBenchEvent_StateChanged> get copyWith => _$FrbBenchEvent_StateChangedCopyWithImpl<FrbBenchEvent_StateChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_StateChanged&&(identical(other.from, from) || other.from == from)&&(identical(other.to, to) || other.to == to));
}


@override
int get hashCode => Object.hash(runtimeType,from,to);

@override
String toString() {
  return 'FrbBenchEvent.stateChanged(from: $from, to: $to)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_StateChangedCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_StateChangedCopyWith(FrbBenchEvent_StateChanged value, $Res Function(FrbBenchEvent_StateChanged) _then) = _$FrbBenchEvent_StateChangedCopyWithImpl;
@useResult
$Res call({
 String from, String to
});




}
/// @nodoc
class _$FrbBenchEvent_StateChangedCopyWithImpl<$Res>
    implements $FrbBenchEvent_StateChangedCopyWith<$Res> {
  _$FrbBenchEvent_StateChangedCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_StateChanged _self;
  final $Res Function(FrbBenchEvent_StateChanged) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? from = null,Object? to = null,}) {
  return _then(FrbBenchEvent_StateChanged(
from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,to: null == to ? _self.to : to // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FrbBenchEvent_Sample extends FrbBenchEvent {
  const FrbBenchEvent_Sample(this.field0): super._();
  

 final  FrbResourceSample field0;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_SampleCopyWith<FrbBenchEvent_Sample> get copyWith => _$FrbBenchEvent_SampleCopyWithImpl<FrbBenchEvent_Sample>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_Sample&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'FrbBenchEvent.sample(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_SampleCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_SampleCopyWith(FrbBenchEvent_Sample value, $Res Function(FrbBenchEvent_Sample) _then) = _$FrbBenchEvent_SampleCopyWithImpl;
@useResult
$Res call({
 FrbResourceSample field0
});




}
/// @nodoc
class _$FrbBenchEvent_SampleCopyWithImpl<$Res>
    implements $FrbBenchEvent_SampleCopyWith<$Res> {
  _$FrbBenchEvent_SampleCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_Sample _self;
  final $Res Function(FrbBenchEvent_Sample) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(FrbBenchEvent_Sample(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as FrbResourceSample,
  ));
}


}

/// @nodoc


class FrbBenchEvent_Token extends FrbBenchEvent {
  const FrbBenchEvent_Token({required this.index, required this.text}): super._();
  

 final  int index;
 final  String text;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_TokenCopyWith<FrbBenchEvent_Token> get copyWith => _$FrbBenchEvent_TokenCopyWithImpl<FrbBenchEvent_Token>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_Token&&(identical(other.index, index) || other.index == index)&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,index,text);

@override
String toString() {
  return 'FrbBenchEvent.token(index: $index, text: $text)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_TokenCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_TokenCopyWith(FrbBenchEvent_Token value, $Res Function(FrbBenchEvent_Token) _then) = _$FrbBenchEvent_TokenCopyWithImpl;
@useResult
$Res call({
 int index, String text
});




}
/// @nodoc
class _$FrbBenchEvent_TokenCopyWithImpl<$Res>
    implements $FrbBenchEvent_TokenCopyWith<$Res> {
  _$FrbBenchEvent_TokenCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_Token _self;
  final $Res Function(FrbBenchEvent_Token) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? text = null,}) {
  return _then(FrbBenchEvent_Token(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FrbBenchEvent_WatchdogWarn extends FrbBenchEvent {
  const FrbBenchEvent_WatchdogWarn(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_WatchdogWarn);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'FrbBenchEvent.watchdogWarn()';
}


}




/// @nodoc


class FrbBenchEvent_WatchdogKill extends FrbBenchEvent {
  const FrbBenchEvent_WatchdogKill(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_WatchdogKill);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'FrbBenchEvent.watchdogKill()';
}


}




/// @nodoc


class FrbBenchEvent_RunFinished extends FrbBenchEvent {
  const FrbBenchEvent_RunFinished({required this.runId, required this.status, this.result}): super._();
  

 final  BigInt runId;
 final  String status;
 final  FrbBenchResult? result;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_RunFinishedCopyWith<FrbBenchEvent_RunFinished> get copyWith => _$FrbBenchEvent_RunFinishedCopyWithImpl<FrbBenchEvent_RunFinished>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_RunFinished&&(identical(other.runId, runId) || other.runId == runId)&&(identical(other.status, status) || other.status == status)&&(identical(other.result, result) || other.result == result));
}


@override
int get hashCode => Object.hash(runtimeType,runId,status,result);

@override
String toString() {
  return 'FrbBenchEvent.runFinished(runId: $runId, status: $status, result: $result)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_RunFinishedCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_RunFinishedCopyWith(FrbBenchEvent_RunFinished value, $Res Function(FrbBenchEvent_RunFinished) _then) = _$FrbBenchEvent_RunFinishedCopyWithImpl;
@useResult
$Res call({
 BigInt runId, String status, FrbBenchResult? result
});




}
/// @nodoc
class _$FrbBenchEvent_RunFinishedCopyWithImpl<$Res>
    implements $FrbBenchEvent_RunFinishedCopyWith<$Res> {
  _$FrbBenchEvent_RunFinishedCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_RunFinished _self;
  final $Res Function(FrbBenchEvent_RunFinished) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,Object? status = null,Object? result = freezed,}) {
  return _then(FrbBenchEvent_RunFinished(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as BigInt,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as String,result: freezed == result ? _self.result : result // ignore: cast_nullable_to_non_nullable
as FrbBenchResult?,
  ));
}


}

/// @nodoc


class FrbBenchEvent_Log extends FrbBenchEvent {
  const FrbBenchEvent_Log({required this.level, required this.message}): super._();
  

 final  String level;
 final  String message;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_LogCopyWith<FrbBenchEvent_Log> get copyWith => _$FrbBenchEvent_LogCopyWithImpl<FrbBenchEvent_Log>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_Log&&(identical(other.level, level) || other.level == level)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,level,message);

@override
String toString() {
  return 'FrbBenchEvent.log(level: $level, message: $message)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_LogCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_LogCopyWith(FrbBenchEvent_Log value, $Res Function(FrbBenchEvent_Log) _then) = _$FrbBenchEvent_LogCopyWithImpl;
@useResult
$Res call({
 String level, String message
});




}
/// @nodoc
class _$FrbBenchEvent_LogCopyWithImpl<$Res>
    implements $FrbBenchEvent_LogCopyWith<$Res> {
  _$FrbBenchEvent_LogCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_Log _self;
  final $Res Function(FrbBenchEvent_Log) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? level = null,Object? message = null,}) {
  return _then(FrbBenchEvent_Log(
level: null == level ? _self.level : level // ignore: cast_nullable_to_non_nullable
as String,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class FrbBenchEvent_Progress extends FrbBenchEvent {
  const FrbBenchEvent_Progress({required this.message}): super._();
  

 final  String message;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$FrbBenchEvent_ProgressCopyWith<FrbBenchEvent_Progress> get copyWith => _$FrbBenchEvent_ProgressCopyWithImpl<FrbBenchEvent_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is FrbBenchEvent_Progress&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'FrbBenchEvent.progress(message: $message)';
}


}

/// @nodoc
abstract mixin class $FrbBenchEvent_ProgressCopyWith<$Res> implements $FrbBenchEventCopyWith<$Res> {
  factory $FrbBenchEvent_ProgressCopyWith(FrbBenchEvent_Progress value, $Res Function(FrbBenchEvent_Progress) _then) = _$FrbBenchEvent_ProgressCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$FrbBenchEvent_ProgressCopyWithImpl<$Res>
    implements $FrbBenchEvent_ProgressCopyWith<$Res> {
  _$FrbBenchEvent_ProgressCopyWithImpl(this._self, this._then);

  final FrbBenchEvent_Progress _self;
  final $Res Function(FrbBenchEvent_Progress) _then;

/// Create a copy of FrbBenchEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(FrbBenchEvent_Progress(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
