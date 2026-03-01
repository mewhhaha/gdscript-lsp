extends Node3D

var _rng: RandomNumberGenerator = RandomNumberGenerator.new()

func _make_rng() -> RandomNumberGenerator:
    return RandomNumberGenerator.new()

func _ready() -> void:
    _rng.randomize()
    _make_rng().randomize()
    queue_free()

func _ambiguous() -> void:
    clear()
