// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'direct.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$BleEvent {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is BleEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'BleEvent()';
}


}

/// @nodoc
class $BleEventCopyWith<$Res>  {
$BleEventCopyWith(BleEvent _, $Res Function(BleEvent) __);
}


/// Adds pattern-matching-related methods to [BleEvent].
extension BleEventPatterns on BleEvent {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( BleEvent_Seen value)?  seen,TResult Function( BleEvent_Invite value)?  invite,required TResult orElse(),}){
final _that = this;
switch (_that) {
case BleEvent_Seen() when seen != null:
return seen(_that);case BleEvent_Invite() when invite != null:
return invite(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( BleEvent_Seen value)  seen,required TResult Function( BleEvent_Invite value)  invite,}){
final _that = this;
switch (_that) {
case BleEvent_Seen():
return seen(_that);case BleEvent_Invite():
return invite(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( BleEvent_Seen value)?  seen,TResult? Function( BleEvent_Invite value)?  invite,}){
final _that = this;
switch (_that) {
case BleEvent_Seen() when seen != null:
return seen(_that);case BleEvent_Invite() when invite != null:
return invite(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String id,  String name,  String fp)?  seen,TResult Function( String json)?  invite,required TResult orElse(),}) {final _that = this;
switch (_that) {
case BleEvent_Seen() when seen != null:
return seen(_that.id,_that.name,_that.fp);case BleEvent_Invite() when invite != null:
return invite(_that.json);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String id,  String name,  String fp)  seen,required TResult Function( String json)  invite,}) {final _that = this;
switch (_that) {
case BleEvent_Seen():
return seen(_that.id,_that.name,_that.fp);case BleEvent_Invite():
return invite(_that.json);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String id,  String name,  String fp)?  seen,TResult? Function( String json)?  invite,}) {final _that = this;
switch (_that) {
case BleEvent_Seen() when seen != null:
return seen(_that.id,_that.name,_that.fp);case BleEvent_Invite() when invite != null:
return invite(_that.json);case _:
  return null;

}
}

}

/// @nodoc


class BleEvent_Seen extends BleEvent {
  const BleEvent_Seen({required this.id, required this.name, required this.fp}): super._();
  

 final  String id;
 final  String name;
 final  String fp;

/// Create a copy of BleEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$BleEvent_SeenCopyWith<BleEvent_Seen> get copyWith => _$BleEvent_SeenCopyWithImpl<BleEvent_Seen>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is BleEvent_Seen&&(identical(other.id, id) || other.id == id)&&(identical(other.name, name) || other.name == name)&&(identical(other.fp, fp) || other.fp == fp));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id,name,fp);
}

@override
String toString() {
    return 'BleEvent.seen(id: $id, name: $name, fp: $fp)';
}


}

/// @nodoc
abstract mixin class $BleEvent_SeenCopyWith<$Res> implements $BleEventCopyWith<$Res> {
  factory $BleEvent_SeenCopyWith(BleEvent_Seen value, $Res Function(BleEvent_Seen) _then) = _$BleEvent_SeenCopyWithImpl;
@useResult
$Res call({
 String id, String name, String fp
});




}
/// @nodoc
class _$BleEvent_SeenCopyWithImpl<$Res>
    implements $BleEvent_SeenCopyWith<$Res> {
  _$BleEvent_SeenCopyWithImpl(this._self, this._then);

  final BleEvent_Seen _self;
  final $Res Function(BleEvent_Seen) _then;

/// Create a copy of BleEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? name = null,Object? fp = null,}) {
  return _then(BleEvent_Seen(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,fp: null == fp ? _self.fp : fp // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class BleEvent_Invite extends BleEvent {
  const BleEvent_Invite({required this.json}): super._();
  

 final  String json;

/// Create a copy of BleEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$BleEvent_InviteCopyWith<BleEvent_Invite> get copyWith => _$BleEvent_InviteCopyWithImpl<BleEvent_Invite>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is BleEvent_Invite&&(identical(other.json, json) || other.json == json));
}


@override
int get hashCode {
    return Object.hash(runtimeType,json);
}

@override
String toString() {
    return 'BleEvent.invite(json: $json)';
}


}

/// @nodoc
abstract mixin class $BleEvent_InviteCopyWith<$Res> implements $BleEventCopyWith<$Res> {
  factory $BleEvent_InviteCopyWith(BleEvent_Invite value, $Res Function(BleEvent_Invite) _then) = _$BleEvent_InviteCopyWithImpl;
@useResult
$Res call({
 String json
});




}
/// @nodoc
class _$BleEvent_InviteCopyWithImpl<$Res>
    implements $BleEvent_InviteCopyWith<$Res> {
  _$BleEvent_InviteCopyWithImpl(this._self, this._then);

  final BleEvent_Invite _self;
  final $Res Function(BleEvent_Invite) _then;

/// Create a copy of BleEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? json = null,}) {
  return _then(BleEvent_Invite(
json: null == json ? _self.json : json // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
