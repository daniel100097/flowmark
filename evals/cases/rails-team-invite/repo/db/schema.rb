ActiveRecord::Schema[7.2].define(version: 2026_10_01_000000) do
  create_table "invitations", force: :cascade do |t|
    t.bigint "team_id", null: false
    t.string "email", null: false
    t.string "token", null: false
    t.bigint "invited_by_id", null: false
    t.bigint "accepted_by_id"
    t.datetime "expires_at", null: false
    t.datetime "accepted_at"
    t.timestamps
    t.index ["token"], unique: true
    t.index ["team_id", "email"], unique: true, where: "accepted_at IS NULL"
  end

  create_table "memberships", force: :cascade do |t|
    t.bigint "team_id", null: false
    t.bigint "user_id", null: false
    t.string "role", null: false
    t.timestamps
    t.index ["team_id", "user_id"], unique: true
  end
end
