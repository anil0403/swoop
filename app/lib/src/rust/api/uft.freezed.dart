// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'uft.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$Incoming {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'Incoming()';
}


}

/// @nodoc
class $IncomingCopyWith<$Res>  {
$IncomingCopyWith(Incoming _, $Res Function(Incoming) __);
}


/// Adds pattern-matching-related methods to [Incoming].
extension IncomingPatterns on Incoming {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( Incoming_Offer value)?  offer,TResult Function( Incoming_Started value)?  started,TResult Function( Incoming_Progress value)?  progress,TResult Function( Incoming_Done value)?  done,TResult Function( Incoming_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case Incoming_Offer() when offer != null:
return offer(_that);case Incoming_Started() when started != null:
return started(_that);case Incoming_Progress() when progress != null:
return progress(_that);case Incoming_Done() when done != null:
return done(_that);case Incoming_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( Incoming_Offer value)  offer,required TResult Function( Incoming_Started value)  started,required TResult Function( Incoming_Progress value)  progress,required TResult Function( Incoming_Done value)  done,required TResult Function( Incoming_Failed value)  failed,}){
final _that = this;
switch (_that) {
case Incoming_Offer():
return offer(_that);case Incoming_Started():
return started(_that);case Incoming_Progress():
return progress(_that);case Incoming_Done():
return done(_that);case Incoming_Failed():
return failed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( Incoming_Offer value)?  offer,TResult? Function( Incoming_Started value)?  started,TResult? Function( Incoming_Progress value)?  progress,TResult? Function( Incoming_Done value)?  done,TResult? Function( Incoming_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case Incoming_Offer() when offer != null:
return offer(_that);case Incoming_Started() when started != null:
return started(_that);case Incoming_Progress() when progress != null:
return progress(_that);case Incoming_Done() when done != null:
return done(_that);case Incoming_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( PlatformInt64 id,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  String code)?  offer,TResult Function( PlatformInt64 session,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  PlatformInt64 resumed,  String folder)?  started,TResult Function( PlatformInt64 session,  PlatformInt64 done,  PlatformInt64 total,  double speed,  double? eta)?  progress,TResult Function( PlatformInt64 session,  PlatformInt64 bytes,  double secs)?  done,TResult Function( PlatformInt64 session,  String error)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case Incoming_Offer() when offer != null:
return offer(_that.id,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.code);case Incoming_Started() when started != null:
return started(_that.session,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.resumed,_that.folder);case Incoming_Progress() when progress != null:
return progress(_that.session,_that.done,_that.total,_that.speed,_that.eta);case Incoming_Done() when done != null:
return done(_that.session,_that.bytes,_that.secs);case Incoming_Failed() when failed != null:
return failed(_that.session,_that.error);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( PlatformInt64 id,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  String code)  offer,required TResult Function( PlatformInt64 session,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  PlatformInt64 resumed,  String folder)  started,required TResult Function( PlatformInt64 session,  PlatformInt64 done,  PlatformInt64 total,  double speed,  double? eta)  progress,required TResult Function( PlatformInt64 session,  PlatformInt64 bytes,  double secs)  done,required TResult Function( PlatformInt64 session,  String error)  failed,}) {final _that = this;
switch (_that) {
case Incoming_Offer():
return offer(_that.id,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.code);case Incoming_Started():
return started(_that.session,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.resumed,_that.folder);case Incoming_Progress():
return progress(_that.session,_that.done,_that.total,_that.speed,_that.eta);case Incoming_Done():
return done(_that.session,_that.bytes,_that.secs);case Incoming_Failed():
return failed(_that.session,_that.error);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( PlatformInt64 id,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  String code)?  offer,TResult? Function( PlatformInt64 session,  String peerName,  String fingerprint,  PlatformInt64 files,  PlatformInt64 total,  PlatformInt64 resumed,  String folder)?  started,TResult? Function( PlatformInt64 session,  PlatformInt64 done,  PlatformInt64 total,  double speed,  double? eta)?  progress,TResult? Function( PlatformInt64 session,  PlatformInt64 bytes,  double secs)?  done,TResult? Function( PlatformInt64 session,  String error)?  failed,}) {final _that = this;
switch (_that) {
case Incoming_Offer() when offer != null:
return offer(_that.id,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.code);case Incoming_Started() when started != null:
return started(_that.session,_that.peerName,_that.fingerprint,_that.files,_that.total,_that.resumed,_that.folder);case Incoming_Progress() when progress != null:
return progress(_that.session,_that.done,_that.total,_that.speed,_that.eta);case Incoming_Done() when done != null:
return done(_that.session,_that.bytes,_that.secs);case Incoming_Failed() when failed != null:
return failed(_that.session,_that.error);case _:
  return null;

}
}

}

/// @nodoc


class Incoming_Offer extends Incoming {
  const Incoming_Offer({required this.id, required this.peerName, required this.fingerprint, required this.files, required this.total, required this.code}): super._();
  

 final  PlatformInt64 id;
 final  String peerName;
 final  String fingerprint;
 final  PlatformInt64 files;
 final  PlatformInt64 total;
 final  String code;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Incoming_OfferCopyWith<Incoming_Offer> get copyWith => _$Incoming_OfferCopyWithImpl<Incoming_Offer>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming_Offer&&(identical(other.id, id) || other.id == id)&&(identical(other.peerName, peerName) || other.peerName == peerName)&&(identical(other.fingerprint, fingerprint) || other.fingerprint == fingerprint)&&(identical(other.files, files) || other.files == files)&&(identical(other.total, total) || other.total == total)&&(identical(other.code, code) || other.code == code));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,peerName,fingerprint,files,total,code);
}

@override
String toString() {
    return 'Incoming.offer(id: $id, peerName: $peerName, fingerprint: $fingerprint, files: $files, total: $total, code: $code)';
}


}

/// @nodoc
abstract mixin class $Incoming_OfferCopyWith<$Res> implements $IncomingCopyWith<$Res> {
  factory $Incoming_OfferCopyWith(Incoming_Offer value, $Res Function(Incoming_Offer) _then) = _$Incoming_OfferCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 id, String peerName, String fingerprint, PlatformInt64 files, PlatformInt64 total, String code
});




}
/// @nodoc
class _$Incoming_OfferCopyWithImpl<$Res>
    implements $Incoming_OfferCopyWith<$Res> {
  _$Incoming_OfferCopyWithImpl(this._self, this._then);

  final Incoming_Offer _self;
  final $Res Function(Incoming_Offer) _then;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? peerName = null,Object? fingerprint = null,Object? files = null,Object? total = null,Object? code = null,}) {
  return _then(Incoming_Offer(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as PlatformInt64,peerName: null == peerName ? _self.peerName : peerName // ignore: cast_nullable_to_non_nullable
as String,fingerprint: null == fingerprint ? _self.fingerprint : fingerprint // ignore: cast_nullable_to_non_nullable
as String,files: null == files ? _self.files : files // ignore: cast_nullable_to_non_nullable
as PlatformInt64,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as PlatformInt64,code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Incoming_Started extends Incoming {
  const Incoming_Started({required this.session, required this.peerName, required this.fingerprint, required this.files, required this.total, required this.resumed, required this.folder}): super._();
  

 final  PlatformInt64 session;
 final  String peerName;
 final  String fingerprint;
 final  PlatformInt64 files;
 final  PlatformInt64 total;
 final  PlatformInt64 resumed;
/// The save folder its files go to.
 final  String folder;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Incoming_StartedCopyWith<Incoming_Started> get copyWith => _$Incoming_StartedCopyWithImpl<Incoming_Started>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming_Started&&(identical(other.session, session) || other.session == session)&&(identical(other.peerName, peerName) || other.peerName == peerName)&&(identical(other.fingerprint, fingerprint) || other.fingerprint == fingerprint)&&(identical(other.files, files) || other.files == files)&&(identical(other.total, total) || other.total == total)&&(identical(other.resumed, resumed) || other.resumed == resumed)&&(identical(other.folder, folder) || other.folder == folder));
}


@override
int get hashCode {
    return Object.hash(runtimeType,session,peerName,fingerprint,files,total,resumed,folder);
}

@override
String toString() {
    return 'Incoming.started(session: $session, peerName: $peerName, fingerprint: $fingerprint, files: $files, total: $total, resumed: $resumed, folder: $folder)';
}


}

/// @nodoc
abstract mixin class $Incoming_StartedCopyWith<$Res> implements $IncomingCopyWith<$Res> {
  factory $Incoming_StartedCopyWith(Incoming_Started value, $Res Function(Incoming_Started) _then) = _$Incoming_StartedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 session, String peerName, String fingerprint, PlatformInt64 files, PlatformInt64 total, PlatformInt64 resumed, String folder
});




}
/// @nodoc
class _$Incoming_StartedCopyWithImpl<$Res>
    implements $Incoming_StartedCopyWith<$Res> {
  _$Incoming_StartedCopyWithImpl(this._self, this._then);

  final Incoming_Started _self;
  final $Res Function(Incoming_Started) _then;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? session = null,Object? peerName = null,Object? fingerprint = null,Object? files = null,Object? total = null,Object? resumed = null,Object? folder = null,}) {
  return _then(Incoming_Started(
session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as PlatformInt64,peerName: null == peerName ? _self.peerName : peerName // ignore: cast_nullable_to_non_nullable
as String,fingerprint: null == fingerprint ? _self.fingerprint : fingerprint // ignore: cast_nullable_to_non_nullable
as String,files: null == files ? _self.files : files // ignore: cast_nullable_to_non_nullable
as PlatformInt64,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as PlatformInt64,resumed: null == resumed ? _self.resumed : resumed // ignore: cast_nullable_to_non_nullable
as PlatformInt64,folder: null == folder ? _self.folder : folder // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Incoming_Progress extends Incoming {
  const Incoming_Progress({required this.session, required this.done, required this.total, required this.speed, this.eta}): super._();
  

 final  PlatformInt64 session;
 final  PlatformInt64 done;
 final  PlatformInt64 total;
 final  double speed;
 final  double? eta;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Incoming_ProgressCopyWith<Incoming_Progress> get copyWith => _$Incoming_ProgressCopyWithImpl<Incoming_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming_Progress&&(identical(other.session, session) || other.session == session)&&(identical(other.done, done) || other.done == done)&&(identical(other.total, total) || other.total == total)&&(identical(other.speed, speed) || other.speed == speed)&&(identical(other.eta, eta) || other.eta == eta));
}


@override
int get hashCode {
    return Object.hash(runtimeType,session,done,total,speed,eta);
}

@override
String toString() {
    return 'Incoming.progress(session: $session, done: $done, total: $total, speed: $speed, eta: $eta)';
}


}

/// @nodoc
abstract mixin class $Incoming_ProgressCopyWith<$Res> implements $IncomingCopyWith<$Res> {
  factory $Incoming_ProgressCopyWith(Incoming_Progress value, $Res Function(Incoming_Progress) _then) = _$Incoming_ProgressCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 session, PlatformInt64 done, PlatformInt64 total, double speed, double? eta
});




}
/// @nodoc
class _$Incoming_ProgressCopyWithImpl<$Res>
    implements $Incoming_ProgressCopyWith<$Res> {
  _$Incoming_ProgressCopyWithImpl(this._self, this._then);

  final Incoming_Progress _self;
  final $Res Function(Incoming_Progress) _then;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? session = null,Object? done = null,Object? total = null,Object? speed = null,Object? eta = freezed,}) {
  return _then(Incoming_Progress(
session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as PlatformInt64,done: null == done ? _self.done : done // ignore: cast_nullable_to_non_nullable
as PlatformInt64,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as PlatformInt64,speed: null == speed ? _self.speed : speed // ignore: cast_nullable_to_non_nullable
as double,eta: freezed == eta ? _self.eta : eta // ignore: cast_nullable_to_non_nullable
as double?,
  ));
}


}

/// @nodoc


class Incoming_Done extends Incoming {
  const Incoming_Done({required this.session, required this.bytes, required this.secs}): super._();
  

 final  PlatformInt64 session;
 final  PlatformInt64 bytes;
 final  double secs;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Incoming_DoneCopyWith<Incoming_Done> get copyWith => _$Incoming_DoneCopyWithImpl<Incoming_Done>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming_Done&&(identical(other.session, session) || other.session == session)&&(identical(other.bytes, bytes) || other.bytes == bytes)&&(identical(other.secs, secs) || other.secs == secs));
}


@override
int get hashCode {
    return Object.hash(runtimeType,session,bytes,secs);
}

@override
String toString() {
    return 'Incoming.done(session: $session, bytes: $bytes, secs: $secs)';
}


}

/// @nodoc
abstract mixin class $Incoming_DoneCopyWith<$Res> implements $IncomingCopyWith<$Res> {
  factory $Incoming_DoneCopyWith(Incoming_Done value, $Res Function(Incoming_Done) _then) = _$Incoming_DoneCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 session, PlatformInt64 bytes, double secs
});




}
/// @nodoc
class _$Incoming_DoneCopyWithImpl<$Res>
    implements $Incoming_DoneCopyWith<$Res> {
  _$Incoming_DoneCopyWithImpl(this._self, this._then);

  final Incoming_Done _self;
  final $Res Function(Incoming_Done) _then;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? session = null,Object? bytes = null,Object? secs = null,}) {
  return _then(Incoming_Done(
session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as PlatformInt64,bytes: null == bytes ? _self.bytes : bytes // ignore: cast_nullable_to_non_nullable
as PlatformInt64,secs: null == secs ? _self.secs : secs // ignore: cast_nullable_to_non_nullable
as double,
  ));
}


}

/// @nodoc


class Incoming_Failed extends Incoming {
  const Incoming_Failed({required this.session, required this.error}): super._();
  

 final  PlatformInt64 session;
 final  String error;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Incoming_FailedCopyWith<Incoming_Failed> get copyWith => _$Incoming_FailedCopyWithImpl<Incoming_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Incoming_Failed&&(identical(other.session, session) || other.session == session)&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode {
    return Object.hash(runtimeType,session,error);
}

@override
String toString() {
    return 'Incoming.failed(session: $session, error: $error)';
}


}

/// @nodoc
abstract mixin class $Incoming_FailedCopyWith<$Res> implements $IncomingCopyWith<$Res> {
  factory $Incoming_FailedCopyWith(Incoming_Failed value, $Res Function(Incoming_Failed) _then) = _$Incoming_FailedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 session, String error
});




}
/// @nodoc
class _$Incoming_FailedCopyWithImpl<$Res>
    implements $Incoming_FailedCopyWith<$Res> {
  _$Incoming_FailedCopyWithImpl(this._self, this._then);

  final Incoming_Failed _self;
  final $Res Function(Incoming_Failed) _then;

/// Create a copy of Incoming
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? session = null,Object? error = null,}) {
  return _then(Incoming_Failed(
session: null == session ? _self.session : session // ignore: cast_nullable_to_non_nullable
as PlatformInt64,error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$Outgoing {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'Outgoing()';
}


}

/// @nodoc
class $OutgoingCopyWith<$Res>  {
$OutgoingCopyWith(Outgoing _, $Res Function(Outgoing) __);
}


/// Adds pattern-matching-related methods to [Outgoing].
extension OutgoingPatterns on Outgoing {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( Outgoing_Code value)?  code,TResult Function( Outgoing_Progress value)?  progress,TResult Function( Outgoing_Done value)?  done,TResult Function( Outgoing_Reconnecting value)?  reconnecting,TResult Function( Outgoing_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case Outgoing_Code() when code != null:
return code(_that);case Outgoing_Progress() when progress != null:
return progress(_that);case Outgoing_Done() when done != null:
return done(_that);case Outgoing_Reconnecting() when reconnecting != null:
return reconnecting(_that);case Outgoing_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( Outgoing_Code value)  code,required TResult Function( Outgoing_Progress value)  progress,required TResult Function( Outgoing_Done value)  done,required TResult Function( Outgoing_Reconnecting value)  reconnecting,required TResult Function( Outgoing_Failed value)  failed,}){
final _that = this;
switch (_that) {
case Outgoing_Code():
return code(_that);case Outgoing_Progress():
return progress(_that);case Outgoing_Done():
return done(_that);case Outgoing_Reconnecting():
return reconnecting(_that);case Outgoing_Failed():
return failed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( Outgoing_Code value)?  code,TResult? Function( Outgoing_Progress value)?  progress,TResult? Function( Outgoing_Done value)?  done,TResult? Function( Outgoing_Reconnecting value)?  reconnecting,TResult? Function( Outgoing_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case Outgoing_Code() when code != null:
return code(_that);case Outgoing_Progress() when progress != null:
return progress(_that);case Outgoing_Done() when done != null:
return done(_that);case Outgoing_Reconnecting() when reconnecting != null:
return reconnecting(_that);case Outgoing_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String code)?  code,TResult Function( PlatformInt64 done,  PlatformInt64 total,  double speed,  double peak,  double? eta,  int streams)?  progress,TResult Function( PlatformInt64 bytes,  PlatformInt64 resumed,  double secs,  double avg,  double peak)?  done,TResult Function( String error)?  reconnecting,TResult Function( String error)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case Outgoing_Code() when code != null:
return code(_that.code);case Outgoing_Progress() when progress != null:
return progress(_that.done,_that.total,_that.speed,_that.peak,_that.eta,_that.streams);case Outgoing_Done() when done != null:
return done(_that.bytes,_that.resumed,_that.secs,_that.avg,_that.peak);case Outgoing_Reconnecting() when reconnecting != null:
return reconnecting(_that.error);case Outgoing_Failed() when failed != null:
return failed(_that.error);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String code)  code,required TResult Function( PlatformInt64 done,  PlatformInt64 total,  double speed,  double peak,  double? eta,  int streams)  progress,required TResult Function( PlatformInt64 bytes,  PlatformInt64 resumed,  double secs,  double avg,  double peak)  done,required TResult Function( String error)  reconnecting,required TResult Function( String error)  failed,}) {final _that = this;
switch (_that) {
case Outgoing_Code():
return code(_that.code);case Outgoing_Progress():
return progress(_that.done,_that.total,_that.speed,_that.peak,_that.eta,_that.streams);case Outgoing_Done():
return done(_that.bytes,_that.resumed,_that.secs,_that.avg,_that.peak);case Outgoing_Reconnecting():
return reconnecting(_that.error);case Outgoing_Failed():
return failed(_that.error);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String code)?  code,TResult? Function( PlatformInt64 done,  PlatformInt64 total,  double speed,  double peak,  double? eta,  int streams)?  progress,TResult? Function( PlatformInt64 bytes,  PlatformInt64 resumed,  double secs,  double avg,  double peak)?  done,TResult? Function( String error)?  reconnecting,TResult? Function( String error)?  failed,}) {final _that = this;
switch (_that) {
case Outgoing_Code() when code != null:
return code(_that.code);case Outgoing_Progress() when progress != null:
return progress(_that.done,_that.total,_that.speed,_that.peak,_that.eta,_that.streams);case Outgoing_Done() when done != null:
return done(_that.bytes,_that.resumed,_that.secs,_that.avg,_that.peak);case Outgoing_Reconnecting() when reconnecting != null:
return reconnecting(_that.error);case Outgoing_Failed() when failed != null:
return failed(_that.error);case _:
  return null;

}
}

}

/// @nodoc


class Outgoing_Code extends Outgoing {
  const Outgoing_Code({required this.code}): super._();
  

 final  String code;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Outgoing_CodeCopyWith<Outgoing_Code> get copyWith => _$Outgoing_CodeCopyWithImpl<Outgoing_Code>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing_Code&&(identical(other.code, code) || other.code == code));
}


@override
int get hashCode {
    return Object.hash(runtimeType,code);
}

@override
String toString() {
    return 'Outgoing.code(code: $code)';
}


}

/// @nodoc
abstract mixin class $Outgoing_CodeCopyWith<$Res> implements $OutgoingCopyWith<$Res> {
  factory $Outgoing_CodeCopyWith(Outgoing_Code value, $Res Function(Outgoing_Code) _then) = _$Outgoing_CodeCopyWithImpl;
@useResult
$Res call({
 String code
});




}
/// @nodoc
class _$Outgoing_CodeCopyWithImpl<$Res>
    implements $Outgoing_CodeCopyWith<$Res> {
  _$Outgoing_CodeCopyWithImpl(this._self, this._then);

  final Outgoing_Code _self;
  final $Res Function(Outgoing_Code) _then;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? code = null,}) {
  return _then(Outgoing_Code(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Outgoing_Progress extends Outgoing {
  const Outgoing_Progress({required this.done, required this.total, required this.speed, required this.peak, this.eta, required this.streams}): super._();
  

 final  PlatformInt64 done;
 final  PlatformInt64 total;
 final  double speed;
 final  double peak;
 final  double? eta;
 final  int streams;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Outgoing_ProgressCopyWith<Outgoing_Progress> get copyWith => _$Outgoing_ProgressCopyWithImpl<Outgoing_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing_Progress&&(identical(other.done, done) || other.done == done)&&(identical(other.total, total) || other.total == total)&&(identical(other.speed, speed) || other.speed == speed)&&(identical(other.peak, peak) || other.peak == peak)&&(identical(other.eta, eta) || other.eta == eta)&&(identical(other.streams, streams) || other.streams == streams));
}


@override
int get hashCode {
    return Object.hash(runtimeType,done,total,speed,peak,eta,streams);
}

@override
String toString() {
    return 'Outgoing.progress(done: $done, total: $total, speed: $speed, peak: $peak, eta: $eta, streams: $streams)';
}


}

/// @nodoc
abstract mixin class $Outgoing_ProgressCopyWith<$Res> implements $OutgoingCopyWith<$Res> {
  factory $Outgoing_ProgressCopyWith(Outgoing_Progress value, $Res Function(Outgoing_Progress) _then) = _$Outgoing_ProgressCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 done, PlatformInt64 total, double speed, double peak, double? eta, int streams
});




}
/// @nodoc
class _$Outgoing_ProgressCopyWithImpl<$Res>
    implements $Outgoing_ProgressCopyWith<$Res> {
  _$Outgoing_ProgressCopyWithImpl(this._self, this._then);

  final Outgoing_Progress _self;
  final $Res Function(Outgoing_Progress) _then;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? done = null,Object? total = null,Object? speed = null,Object? peak = null,Object? eta = freezed,Object? streams = null,}) {
  return _then(Outgoing_Progress(
done: null == done ? _self.done : done // ignore: cast_nullable_to_non_nullable
as PlatformInt64,total: null == total ? _self.total : total // ignore: cast_nullable_to_non_nullable
as PlatformInt64,speed: null == speed ? _self.speed : speed // ignore: cast_nullable_to_non_nullable
as double,peak: null == peak ? _self.peak : peak // ignore: cast_nullable_to_non_nullable
as double,eta: freezed == eta ? _self.eta : eta // ignore: cast_nullable_to_non_nullable
as double?,streams: null == streams ? _self.streams : streams // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class Outgoing_Done extends Outgoing {
  const Outgoing_Done({required this.bytes, required this.resumed, required this.secs, required this.avg, required this.peak}): super._();
  

 final  PlatformInt64 bytes;
 final  PlatformInt64 resumed;
 final  double secs;
 final  double avg;
 final  double peak;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Outgoing_DoneCopyWith<Outgoing_Done> get copyWith => _$Outgoing_DoneCopyWithImpl<Outgoing_Done>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing_Done&&(identical(other.bytes, bytes) || other.bytes == bytes)&&(identical(other.resumed, resumed) || other.resumed == resumed)&&(identical(other.secs, secs) || other.secs == secs)&&(identical(other.avg, avg) || other.avg == avg)&&(identical(other.peak, peak) || other.peak == peak));
}


@override
int get hashCode {
    return Object.hash(runtimeType,bytes,resumed,secs,avg,peak);
}

@override
String toString() {
    return 'Outgoing.done(bytes: $bytes, resumed: $resumed, secs: $secs, avg: $avg, peak: $peak)';
}


}

/// @nodoc
abstract mixin class $Outgoing_DoneCopyWith<$Res> implements $OutgoingCopyWith<$Res> {
  factory $Outgoing_DoneCopyWith(Outgoing_Done value, $Res Function(Outgoing_Done) _then) = _$Outgoing_DoneCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 bytes, PlatformInt64 resumed, double secs, double avg, double peak
});




}
/// @nodoc
class _$Outgoing_DoneCopyWithImpl<$Res>
    implements $Outgoing_DoneCopyWith<$Res> {
  _$Outgoing_DoneCopyWithImpl(this._self, this._then);

  final Outgoing_Done _self;
  final $Res Function(Outgoing_Done) _then;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? bytes = null,Object? resumed = null,Object? secs = null,Object? avg = null,Object? peak = null,}) {
  return _then(Outgoing_Done(
bytes: null == bytes ? _self.bytes : bytes // ignore: cast_nullable_to_non_nullable
as PlatformInt64,resumed: null == resumed ? _self.resumed : resumed // ignore: cast_nullable_to_non_nullable
as PlatformInt64,secs: null == secs ? _self.secs : secs // ignore: cast_nullable_to_non_nullable
as double,avg: null == avg ? _self.avg : avg // ignore: cast_nullable_to_non_nullable
as double,peak: null == peak ? _self.peak : peak // ignore: cast_nullable_to_non_nullable
as double,
  ));
}


}

/// @nodoc


class Outgoing_Reconnecting extends Outgoing {
  const Outgoing_Reconnecting({required this.error}): super._();
  

 final  String error;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Outgoing_ReconnectingCopyWith<Outgoing_Reconnecting> get copyWith => _$Outgoing_ReconnectingCopyWithImpl<Outgoing_Reconnecting>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing_Reconnecting&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode {
    return Object.hash(runtimeType,error);
}

@override
String toString() {
    return 'Outgoing.reconnecting(error: $error)';
}


}

/// @nodoc
abstract mixin class $Outgoing_ReconnectingCopyWith<$Res> implements $OutgoingCopyWith<$Res> {
  factory $Outgoing_ReconnectingCopyWith(Outgoing_Reconnecting value, $Res Function(Outgoing_Reconnecting) _then) = _$Outgoing_ReconnectingCopyWithImpl;
@useResult
$Res call({
 String error
});




}
/// @nodoc
class _$Outgoing_ReconnectingCopyWithImpl<$Res>
    implements $Outgoing_ReconnectingCopyWith<$Res> {
  _$Outgoing_ReconnectingCopyWithImpl(this._self, this._then);

  final Outgoing_Reconnecting _self;
  final $Res Function(Outgoing_Reconnecting) _then;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? error = null,}) {
  return _then(Outgoing_Reconnecting(
error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class Outgoing_Failed extends Outgoing {
  const Outgoing_Failed({required this.error}): super._();
  

 final  String error;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$Outgoing_FailedCopyWith<Outgoing_Failed> get copyWith => _$Outgoing_FailedCopyWithImpl<Outgoing_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is Outgoing_Failed&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode {
    return Object.hash(runtimeType,error);
}

@override
String toString() {
    return 'Outgoing.failed(error: $error)';
}


}

/// @nodoc
abstract mixin class $Outgoing_FailedCopyWith<$Res> implements $OutgoingCopyWith<$Res> {
  factory $Outgoing_FailedCopyWith(Outgoing_Failed value, $Res Function(Outgoing_Failed) _then) = _$Outgoing_FailedCopyWithImpl;
@useResult
$Res call({
 String error
});




}
/// @nodoc
class _$Outgoing_FailedCopyWithImpl<$Res>
    implements $Outgoing_FailedCopyWith<$Res> {
  _$Outgoing_FailedCopyWithImpl(this._self, this._then);

  final Outgoing_Failed _self;
  final $Res Function(Outgoing_Failed) _then;

/// Create a copy of Outgoing
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? error = null,}) {
  return _then(Outgoing_Failed(
error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
