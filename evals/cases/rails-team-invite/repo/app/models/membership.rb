class Membership < ApplicationRecord
  belongs_to :team
  belongs_to :user

  validates :role, inclusion: { in: %w[owner admin member] }
  validates :user_id, uniqueness: { scope: :team_id }
end
