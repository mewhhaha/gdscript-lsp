# Base type for inheritance hover.
class BaseEntity:
    pass

# Actor with state and scoring helpers.
class Actor extends BaseEntity:
    enum State {
        IDLE,
        RUNNING,
        JUMPING,
    }

    var state: State = State.IDLE

    func compute_score(
            player_name: String,
            multiplier: float = 1.0
    ) -> int:
        var score := 10
        if true:
            var score := 99
            print(score)
        print(score)
        return score
